CREATE DATABASE IF NOT EXISTS platform_analytics;

CREATE TABLE IF NOT EXISTS platform_analytics.platform_events
(
    event_id UUID,
    event_type String,
    topic String,
    payload String,
    occurred_at DateTime64(3, 'UTC') DEFAULT now64(3)
)
ENGINE = MergeTree
ORDER BY (occurred_at, event_type);

CREATE TABLE IF NOT EXISTS platform_analytics.api_request_logs
(
    request_id UUID,
    method String,
    path String,
    status_code UInt16,
    duration_ms UInt32,
    payload String,
    occurred_at DateTime64(3, 'UTC') DEFAULT now64(3)
)
ENGINE = MergeTree
ORDER BY (occurred_at, path);

CREATE TABLE IF NOT EXISTS platform_analytics.notification_delivery_logs
(
    notification_id UUID,
    channel String,
    user_id String,
    status String,
    payload String,
    occurred_at DateTime64(3, 'UTC') DEFAULT now64(3)
)
ENGINE = MergeTree
ORDER BY (occurred_at, channel);
