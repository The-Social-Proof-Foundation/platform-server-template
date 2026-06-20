use crate::parsers::post_events::PostCreatedPayload;

pub fn is_post_created_for_platform(platform_id: &str, data: &PostCreatedPayload) -> bool {
    data.platform_id.eq_ignore_ascii_case(platform_id)
}

pub fn matches_package(package_id: Option<&str>, configured: Option<&str>) -> bool {
    match (package_id, configured) {
        (_, None) => true,
        (Some(actual), Some(expected)) => actual.eq_ignore_ascii_case(expected),
        (None, Some(_)) => false,
    }
}
