use std::collections::HashMap;

use crate::types::{FormatJSMessage, IntlFile};

#[derive(Debug)]
pub enum ParseError {
    Json(serde_json::Error),
    Yaml(serde_yaml::Error),
    UnknownFormat(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Json(e) => write!(f, "JSON parse error: {e}"),
            ParseError::Yaml(e) => write!(f, "YAML parse error: {e}"),
            ParseError::UnknownFormat(s) => write!(f, "Unknown format: {s}"),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_json(s: &str) -> Result<IntlFile, ParseError> {
    // Try FormatJS first — values are objects containing `defaultMessage`
    if let Ok(m) = serde_json::from_str::<HashMap<String, FormatJSMessage>>(s) {
        if !m.is_empty() {
            return Ok(IntlFile::FormatJS(m));
        }
    }
    // Fall back to a flat string map
    serde_json::from_str::<HashMap<String, String>>(s)
        .map(IntlFile::Simple)
        .map_err(ParseError::Json)
}

pub fn parse_yaml(s: &str) -> Result<IntlFile, ParseError> {
    let root: serde_yaml::Value = serde_yaml::from_str(s).map_err(ParseError::Yaml)?;

    let mapping = root
        .as_mapping()
        .ok_or(ParseError::UnknownFormat("expected a YAML mapping at top level"))?;

    // Rails format: exactly one top-level key whose value is a flat string map
    if mapping.len() == 1 {
        let (k, v) = mapping.iter().next().unwrap();
        if let (Some(locale), Some(inner)) = (k.as_str(), v.as_mapping()) {
            if inner.values().all(|v| v.as_str().is_some()) {
                let map = yaml_mapping_to_strings(inner);
                return Ok(IntlFile::Rails(locale.to_string(), map));
            }
        }
    }

    // Plain flat string map
    if mapping.values().all(|v| v.as_str().is_some()) {
        return Ok(IntlFile::Simple(yaml_mapping_to_strings(mapping)));
    }

    Err(ParseError::UnknownFormat(
        "YAML structure is not a recognised Rails or Simple format",
    ))
}

fn yaml_mapping_to_strings(m: &serde_yaml::Mapping) -> HashMap<String, String> {
    m.iter()
        .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
        .collect()
}

/// Detect the format from the file extension and parse accordingly.
pub fn parse_input(s: &str, filename: &str) -> Result<IntlFile, ParseError> {
    if filename.ends_with(".yaml") || filename.ends_with(".yml") {
        parse_yaml(s)
    } else {
        parse_json(s)
    }
}

/// Serialise an `IntlFile` back to its wire format.
/// Simple / FormatJS → pretty JSON; Rails → YAML with locale wrapper.
pub fn serialise(file: &IntlFile) -> Result<String, serde_json::Error> {
    match file {
        IntlFile::Simple(m) => serde_json::to_string_pretty(m),
        IntlFile::FormatJS(m) => serde_json::to_string_pretty(m),
        IntlFile::Rails(locale, m) => {
            // Build  `locale:\n  key: value` YAML by hand via serde_yaml::Value
            let inner: serde_yaml::Mapping = m
                .iter()
                .map(|(k, v)| {
                    (
                        serde_yaml::Value::String(k.clone()),
                        serde_yaml::Value::String(v.clone()),
                    )
                })
                .collect();
            let mut outer = serde_yaml::Mapping::new();
            outer.insert(
                serde_yaml::Value::String(locale.clone()),
                serde_yaml::Value::Mapping(inner),
            );
            // serde_yaml::to_string can't fail on plain string mappings,
            // so we unwrap and return a dummy error-free value via a small trick.
            Ok(serde_yaml::to_string(&serde_yaml::Value::Mapping(outer))
                .expect("serialising a plain string YAML mapping should never fail"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- JSON: Simple ---

    #[test]
    fn test_parse_json_simple() {
        let s = r#"{"greeting": "Hello", "farewell": "Goodbye"}"#;
        let IntlFile::Simple(map) = parse_json(s).unwrap() else { panic!("expected Simple") };
        assert_eq!(map["greeting"], "Hello");
        assert_eq!(map["farewell"], "Goodbye");
    }

    // --- JSON: FormatJS ---

    #[test]
    fn test_parse_json_formatjs_with_description() {
        let s = r#"{"abc123": {"defaultMessage": "Hello World", "description": "A greeting"}}"#;
        let IntlFile::FormatJS(map) = parse_json(s).unwrap() else { panic!("expected FormatJS") };
        assert_eq!(map["abc123"].default_message, "Hello World");
        assert_eq!(map["abc123"].description.as_deref(), Some("A greeting"));
    }

    #[test]
    fn test_parse_json_formatjs_without_description() {
        let s = r#"{"2RFWLf": {"defaultMessage": "Speedtest"}}"#;
        let IntlFile::FormatJS(map) = parse_json(s).unwrap() else { panic!("expected FormatJS") };
        assert_eq!(map["2RFWLf"].default_message, "Speedtest");
        assert!(map["2RFWLf"].description.is_none());
    }

    #[test]
    fn test_parse_json_formatjs_real_file() {
        let s = r#"{
            "2RFWLf": {"defaultMessage": "Speedtest"},
            "9QV3cp": {"defaultMessage": "Available IP Blocks"},
            "qq7WMq": {"defaultMessage": "All VPS come with 1x IPv4 and 1x IPv6 address and unmetered traffic, all prices are excluding taxes."}
        }"#;
        let IntlFile::FormatJS(map) = parse_json(s).unwrap() else { panic!("expected FormatJS") };
        assert_eq!(map.len(), 3);
        assert_eq!(map["9QV3cp"].default_message, "Available IP Blocks");
    }

    // --- YAML: Simple ---

    #[test]
    fn test_parse_yaml_simple() {
        let s = "greeting: Hello\nfarewell: Goodbye\n";
        let IntlFile::Simple(map) = parse_yaml(s).unwrap() else { panic!("expected Simple") };
        assert_eq!(map["greeting"], "Hello");
        assert_eq!(map["farewell"], "Goodbye");
    }

    // --- YAML: Rails ---

    #[test]
    fn test_parse_yaml_rails_en() {
        let s = "en:\n  greeting: Hello\n  farewell: Goodbye\n";
        let IntlFile::Rails(locale, map) = parse_yaml(s).unwrap() else { panic!("expected Rails") };
        assert_eq!(locale, "en");
        assert_eq!(map["greeting"], "Hello");
        assert_eq!(map["farewell"], "Goodbye");
    }

    #[test]
    fn test_parse_yaml_rails_preserves_locale_key() {
        let s = "de:\n  greeting: Hallo\n";
        let IntlFile::Rails(locale, _) = parse_yaml(s).unwrap() else { panic!("expected Rails") };
        assert_eq!(locale, "de");
    }

    // --- Format detection via extension ---

    #[test]
    fn test_parse_input_json_simple() {
        let s = r#"{"key": "value"}"#;
        assert!(matches!(parse_input(s, "messages.json").unwrap(), IntlFile::Simple(_)));
    }

    #[test]
    fn test_parse_input_json_formatjs() {
        let s = r#"{"k": {"defaultMessage": "v"}}"#;
        assert!(matches!(parse_input(s, "en.json").unwrap(), IntlFile::FormatJS(_)));
    }

    #[test]
    fn test_parse_input_yml() {
        let s = "key: value\n";
        assert!(matches!(parse_input(s, "messages.yml").unwrap(), IntlFile::Simple(_)));
    }

    #[test]
    fn test_parse_input_yaml_rails() {
        let s = "en:\n  key: value\n";
        assert!(matches!(parse_input(s, "en.yaml").unwrap(), IntlFile::Rails(_, _)));
    }

    // --- Serialisation round-trips ---

    #[test]
    fn test_roundtrip_simple_json() {
        let file = parse_json(r#"{"hello": "world"}"#).unwrap();
        let IntlFile::Simple(map) = parse_json(&serialise(&file).unwrap()).unwrap() else { panic!() };
        assert_eq!(map["hello"], "world");
    }

    #[test]
    fn test_roundtrip_formatjs_preserves_description() {
        let s = r#"{"k": {"defaultMessage": "Hello", "description": "a hint"}}"#;
        let out = serialise(&parse_json(s).unwrap()).unwrap();
        assert!(out.contains("a hint"), "description should survive serialisation");
        assert!(out.contains("defaultMessage"), "key should be camelCase");
        let IntlFile::FormatJS(map) = parse_json(&out).unwrap() else { panic!() };
        assert_eq!(map["k"].default_message, "Hello");
        assert_eq!(map["k"].description.as_deref(), Some("a hint"));
    }

    #[test]
    fn test_roundtrip_rails_yaml() {
        let s = "en:\n  greeting: Hello\n  farewell: Goodbye\n";
        let out = serialise(&parse_yaml(s).unwrap()).unwrap();
        let IntlFile::Rails(locale, map) = parse_yaml(&out).unwrap() else { panic!() };
        assert_eq!(locale, "en");
        assert_eq!(map["greeting"], "Hello");
        assert_eq!(map["farewell"], "Goodbye");
    }

    #[test]
    fn test_roundtrip_simple_yaml() {
        let s = "greeting: Hello\n";
        let out = serialise(&parse_yaml(s).unwrap()).unwrap();
        let IntlFile::Simple(map) = parse_yaml(&out).unwrap() else { panic!() };
        assert_eq!(map["greeting"], "Hello");
    }
}
