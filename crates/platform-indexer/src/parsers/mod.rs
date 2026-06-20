pub mod blocking_events;
pub mod post_events;
pub mod social_graph_events;

pub use post_events::{ParsedChainEvent, RawGrpcEvent};

use crate::filters::platform_filter::matches_social_package;

pub fn parse_grpc_event(raw: &RawGrpcEvent) -> Option<ParsedChainEvent> {
    if !matches_social_package(raw.package_id.as_deref()) {
        return None;
    }
    match raw.module.as_deref() {
        Some("post") => post_events::parse_post_event(raw),
        Some("social_graph") => social_graph_events::parse(raw),
        Some("block_list") | Some("blocking") => blocking_events::parse(raw),
        _ => None,
    }
}
