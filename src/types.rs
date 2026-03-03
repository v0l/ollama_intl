use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// FormatJS / React-Intl compiled message descriptor.
/// JSON format: `{ "id": { "defaultMessage": "...", "description": "..." } }`
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct FormatJSMessage {
    #[serde(rename = "defaultMessage")]
    pub default_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The three well-known i18n message formats we support.
#[derive(Debug, Clone)]
pub enum IntlFile {
    /// Flat key→string map (i18next, Chrome extensions, etc.)
    /// JSON: `{ "key": "value" }`  /  YAML: `key: value`
    Simple(HashMap<String, String>),

    /// FormatJS / React-Intl compiled output.
    /// JSON: `{ "id": { "defaultMessage": "...", "description": "..." } }`
    FormatJS(HashMap<String, FormatJSMessage>),

    /// Rails / Ruby i18n YAML with a top-level locale wrapper.
    /// YAML: `en:\n  key: value`
    Rails(String, HashMap<String, String>),
}
