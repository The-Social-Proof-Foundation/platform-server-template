use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SettingDefinition {
    pub key: &'static str,
    pub default_value: Option<&'static str>,
    pub description: Option<&'static str>,
}

pub const SETTING_DEFINITIONS: &[SettingDefinition] = &[
    SettingDefinition {
        key: "notify.push.enabled",
        default_value: Some("true"),
        description: Some("Enable push notifications (APNs + FCM)"),
    },
    SettingDefinition {
        key: "notify.email.enabled",
        default_value: Some("true"),
        description: Some("Enable email notifications via Resend"),
    },
    SettingDefinition {
        key: "notify.mentions",
        default_value: Some("true"),
        description: Some("Notify when mentioned in a post"),
    },
    SettingDefinition {
        key: "notify.comments",
        default_value: Some("true"),
        description: Some("Notify on new comments"),
    },
    SettingDefinition {
        key: "notify.likes",
        default_value: Some("true"),
        description: Some("Notify on likes"),
    },
    SettingDefinition {
        key: "notify.referrals",
        default_value: Some("true"),
        description: Some("Notify on referral rewards and claims"),
    },
    SettingDefinition {
        key: "content.nsfw.allow",
        default_value: Some("false"),
        description: Some("Include NSFW content in search and discovery results"),
    },
    SettingDefinition {
        key: "referral.reward.claimed",
        default_value: Some("false"),
        description: Some("Whether referral reward has been claimed"),
    },
];

pub fn known_keys() -> impl Iterator<Item = &'static str> {
    SETTING_DEFINITIONS.iter().map(|d| d.key)
}

pub fn default_for(key: &str) -> Option<&str> {
    SETTING_DEFINITIONS
        .iter()
        .find(|d| d.key == key)
        .and_then(|d| d.default_value)
}

pub fn parse_bool(value: &str, fallback: bool) -> bool {
    parse_bool_default(value).unwrap_or(fallback)
}

pub fn parse_bool_default(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

pub fn parse_string<'a>(value: Option<&'a str>, fallback: &'a str) -> &'a str {
    value.filter(|v| !v.is_empty()).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_coerces_common_values() {
        assert!(parse_bool("true", false));
        assert!(parse_bool("1", false));
        assert!(parse_bool("yes", false));
        assert!(!parse_bool("false", true));
        assert!(!parse_bool("0", true));
        assert!(!parse_bool("no", true));
        assert!(parse_bool("maybe", true));
        assert!(!parse_bool("maybe", false));
    }

    #[test]
    fn catalog_includes_notification_prefs() {
        assert!(default_for("notify.push.enabled").is_some());
        assert!(default_for("notify.mentions").is_some());
    }
}
