use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A normalised message entry used across all file formats.
/// `text` is the string to translate; `note` is optional metadata
/// passed through verbatim (e.g. FormatJS `description`).
#[derive(Debug, Clone)]
pub struct FormattedMessage {
    pub text: String,
    pub note: Option<String>,
}

/// Wire representation of a FormatJS / React-Intl message descriptor.
/// Used only at the parse/serialise boundary in `parse.rs`.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FormatJSWire {
    #[serde(rename = "defaultMessage")]
    pub default_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The three well-known i18n message formats we support.
/// All variants store `HashMap<String, FormattedMessage>` internally.
#[derive(Debug, Clone)]
pub enum IntlFile {
    /// Flat key→string map (i18next, Chrome extensions, etc.)
    Simple(HashMap<String, FormattedMessage>),

    /// FormatJS / React-Intl compiled output.
    FormatJS(HashMap<String, FormattedMessage>),

    /// Rails / Ruby i18n YAML with a top-level locale wrapper.
    Rails(String, HashMap<String, FormattedMessage>),
}

impl IntlFile {
    pub fn messages(&self) -> &HashMap<String, FormattedMessage> {
        match self {
            IntlFile::Simple(m) | IntlFile::FormatJS(m) | IntlFile::Rails(_, m) => m,
        }
    }

    pub fn messages_mut(&mut self) -> &mut HashMap<String, FormattedMessage> {
        match self {
            IntlFile::Simple(m) | IntlFile::FormatJS(m) | IntlFile::Rails(_, m) => m,
        }
    }

    pub fn len(&self) -> usize {
        self.messages().len()
    }
}

impl<'a> IntoIterator for &'a IntlFile {
    type Item = (&'a String, &'a FormattedMessage);
    type IntoIter = std::collections::hash_map::Iter<'a, String, FormattedMessage>;

    fn into_iter(self) -> Self::IntoIter {
        self.messages().iter()
    }
}
