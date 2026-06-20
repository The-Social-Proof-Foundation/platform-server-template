pub const MYSO_SOCIAL_PACKAGE_ID: &str = "0x50c1";

use crate::parsers::post_events::PostCreatedPayload;

pub fn is_post_created_for_platform(platform_id: &str, data: &PostCreatedPayload) -> bool {
    data.platform_id.eq_ignore_ascii_case(platform_id)
}

pub fn matches_social_package(package_id: Option<&str>) -> bool {
    package_id.is_some_and(|id| id.eq_ignore_ascii_case(MYSO_SOCIAL_PACKAGE_ID))
}
