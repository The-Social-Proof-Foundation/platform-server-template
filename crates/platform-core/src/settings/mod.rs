use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SettingDefinition {
    pub key: &'static str,
    pub default_value: Option<&'static str>,
    pub description: Option<&'static str>,
}

/// Fork: add entries here, e.g. `SettingDefinition { key: "theme", default_value: Some("system"), description: Some("UI theme") }`
pub const SETTING_DEFINITIONS: &[SettingDefinition] = &[];

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
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => fallback,
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
    fn catalog_starts_empty() {
        assert_eq!(SETTING_DEFINITIONS.len(), 0);
        assert!(default_for("theme").is_none());
    }
}
