use futures::StreamExt;
use myso_rpc::field::{FieldMask, FieldMaskUtil};
use myso_rpc::proto::myso::rpc::v2::Checkpoint;
use myso_rpc::proto::myso::rpc::v2::SubscribeCheckpointsRequest;
use platform_notify::NotificationService;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::config::IndexerConfig;
use crate::cursor::{get_checkpoint_cursor, record_chain_event, set_checkpoint_cursor};
use crate::grpc::client::create_client;
use crate::handlers::dispatcher::{handle_parsed_event, EventMeta};
use crate::metrics::SharedIndexerMetrics;
use crate::parsers::post_events::{parse_grpc_event, RawGrpcEvent};

fn prost_json_to_serde(value: &prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match &value.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::NumberValue(n)) => serde_json::json!(*n),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::StructValue(s)) => {
            let mut map = serde_json::Map::new();
            for (k, v) in &s.fields {
                map.insert(k.clone(), prost_json_to_serde(v));
            }
            serde_json::Value::Object(map)
        }
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(prost_json_to_serde).collect())
        }
        None => serde_json::Value::Null,
    }
}

pub async fn process_checkpoint(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    platform_id: &str,
    checkpoint: myso_rpc::proto::myso::rpc::v2::Checkpoint,
    metrics: &SharedIndexerMetrics,
) -> platform_core::AppResult<()> {
    let Some(seq) = checkpoint.sequence_number_opt() else {
        return Ok(());
    };
    let seq_i64 = seq as i64;
    let tx_events = collect_tx_events(&checkpoint);

    for (tx_digest, event_index, raw) in tx_events {
        let Some(parsed) = parse_grpc_event(&raw) else {
            metrics.events_skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            continue;
        };

        let kind = serde_json::to_value(&parsed)
            .ok()
            .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
            .unwrap_or_else(|| "Unknown".into());

        let inserted = record_chain_event(
            pool,
            &tx_digest,
            event_index,
            seq_i64,
            &kind,
            serde_json::to_value(&parsed).unwrap_or_default(),
        )
        .await?;

        if !inserted {
            metrics.events_skipped.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            continue;
        }

        if let Err(err) = handle_parsed_event(
            pool,
            redis,
            notify,
            platform_id,
            parsed,
            EventMeta {
                tx_digest,
                checkpoint_seq: seq_i64,
            },
        )
        .await
        {
            let msg = err.to_string();
            if let Ok(mut guard) = metrics.last_error.lock() {
                *guard = Some(msg.clone());
            }
            error!(error = %msg, "indexer handler failed");
        } else {
            metrics.events_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    set_checkpoint_cursor(pool, seq_i64).await?;
    metrics.checkpoints_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut guard) = metrics.last_checkpoint_seq.lock() {
        *guard = Some(seq.to_string());
    }
    Ok(())
}

fn collect_tx_events(
    checkpoint: &myso_rpc::proto::myso::rpc::v2::Checkpoint,
) -> Vec<(String, i32, RawGrpcEvent)> {
    let mut out = Vec::new();
    for tx in &checkpoint.transactions {
        let tx_digest = tx.digest_opt().unwrap_or("unknown").to_string();
        let events = tx.events.as_ref().map(|e| e.events.as_slice()).unwrap_or(&[]);
        for (event_index, ev) in events.iter().enumerate() {
            let json = ev
                .json_opt()
                .map(prost_json_to_serde)
                .unwrap_or(serde_json::Value::Null);
            out.push((
                tx_digest.clone(),
                event_index as i32,
                RawGrpcEvent {
                    package_id: ev.package_id_opt().map(str::to_string),
                    module: ev.module_opt().map(str::to_string),
                    sender: ev.sender_opt().map(str::to_string),
                    event_type: ev.event_type_opt().map(str::to_string),
                    json,
                },
            ));
        }
    }
    out
}

pub async fn fetch_checkpoint_with_transactions(
    grpc_url: &str,
    sequence_number: u64,
) -> Result<Option<myso_rpc::proto::myso::rpc::v2::Checkpoint>, tonic::Status> {
    let mut client = create_client(grpc_url)?;
    let read_mask = FieldMask::from_paths([
        Checkpoint::path_builder().sequence_number(),
        Checkpoint::path_builder().transactions().digest(),
        Checkpoint::path_builder().transactions().events().finish(),
    ]);
    let response = client
        .ledger_client()
        .get_checkpoint(
            myso_rpc::proto::myso::rpc::v2::GetCheckpointRequest::default()
                .with_sequence_number(sequence_number)
                .with_read_mask(read_mask),
        )
        .await?;
    Ok(response.into_inner().checkpoint)
}

pub async fn catch_up_checkpoints(
    pool: &PgPool,
    redis: &mut ConnectionManager,
    notify: &NotificationService,
    config: &IndexerConfig,
    metrics: &SharedIndexerMetrics,
    from_seq: u64,
    to_seq: u64,
) -> platform_core::AppResult<()> {
    for seq in from_seq..=to_seq {
        if let Some(cp) = fetch_checkpoint_with_transactions(&config.grpc_url, seq)
            .await
            .map_err(|e| platform_core::AppError::Internal(e.to_string()))?
        {
            process_checkpoint(pool, redis, notify, &config.platform_id, cp, metrics).await?;
        }
    }
    Ok(())
}

pub struct IndexerHandle {
    stop: tokio::sync::watch::Sender<bool>,
}

impl IndexerHandle {
    pub fn stop(&self) {
        let _ = self.stop.send(true);
    }
}

pub fn spawn_indexer(
    pool: PgPool,
    mut redis: ConnectionManager,
    notify: NotificationService,
    config: IndexerConfig,
    metrics: SharedIndexerMetrics,
) -> IndexerHandle {
    let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        let mut last_processed = get_checkpoint_cursor(&pool).await.ok().flatten().map(|s| s as u64);

        info!(
            platform_id = %config.platform_id,
            cursor = ?last_processed,
            "starting checkpoint stream"
        );

        let read_mask = FieldMask::from_paths([
            Checkpoint::path_builder().sequence_number(),
            Checkpoint::path_builder().transactions().digest(),
            Checkpoint::path_builder().transactions().events().finish(),
        ]);

        while !*stop_rx.borrow() {
            let mut client = match create_client(&config.grpc_url) {
                Ok(c) => c,
                Err(err) => {
                    warn!(error = %err, "failed to create grpc client; retrying in 3s");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                }
            };

            let subscription = match client
                .subscription_client()
                .subscribe_checkpoints(
                    SubscribeCheckpointsRequest::default().with_read_mask(read_mask.clone()),
                )
                .await
            {
                Ok(s) => s.into_inner(),
                Err(err) => {
                    warn!(error = %err, "failed to subscribe; retrying in 3s");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                }
            };

            tokio::pin!(subscription);

            while !*stop_rx.borrow() {
                let item = tokio::select! {
                    _ = stop_rx.changed() => break,
                    item = subscription.next() => item,
                };

                let Some(item) = item else { break };
                let response = match item {
                    Ok(r) => r,
                    Err(err) => {
                        warn!(error = %err, "stream error");
                        break;
                    }
                };

                let Some(mut checkpoint) = response.checkpoint else { continue };
                let Some(seq) = checkpoint.sequence_number_opt() else { continue };

                if let Some(last) = last_processed {
                    if seq <= last {
                        continue;
                    }
                    if seq > last + 1 {
                        warn!(from = last + 1, to = seq - 1, "gap detected; catching up");
                        let _ = catch_up_checkpoints(
                            &pool,
                            &mut redis,
                            &notify,
                            &config,
                            &metrics,
                            last + 1,
                            seq - 1,
                        )
                        .await;
                    }
                }

                if checkpoint.transactions.is_empty() {
                    if let Ok(Some(full)) =
                        fetch_checkpoint_with_transactions(&config.grpc_url, seq).await
                    {
                        checkpoint = full;
                    }
                }

                if let Err(err) = process_checkpoint(
                    &pool,
                    &mut redis,
                    &notify,
                    &config.platform_id,
                    checkpoint,
                    &metrics,
                )
                .await
                {
                    error!(error = %err, "checkpoint processing failed");
                }
                last_processed = Some(seq);
            }

            if *stop_rx.borrow() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }

        info!("indexer stopped");
    });

    IndexerHandle { stop: stop_tx }
}
