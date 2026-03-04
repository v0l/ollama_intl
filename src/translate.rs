use std::sync::Arc;

use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, CreateChatCompletionRequest,
    },
    Client,
};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::types::{FormattedMessage, IntlFile};

#[derive(Clone)]
pub struct TranslateConfig {
    pub url: String,
    pub model: String,
    pub api_key: String,
    pub source_lang: String,
    pub source_code: String,
    pub target_lang: String,
    pub target_code: String,
}

/// Extract all `{expr}` placeholders from a string, ignoring escaped `{{` / `}}`.
/// Handles nested braces (e.g. ICU plural expressions).
/// Returns each top-level `{expr}` as a complete string including the braces.
pub fn simple_placeholders(s: &str) -> Vec<String> {
    let mut vars = vec![];
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                // Escaped `{{` — skip both
                chars.next();
                continue;
            }
            // Collect until the matching `}`, tracking nesting depth
            let mut inner = String::new();
            let mut depth = 1usize;
            while let Some(nc) = chars.next() {
                match nc {
                    '{' => { depth += 1; inner.push(nc); }
                    '}' => {
                        depth -= 1;
                        if depth == 0 { break; }
                        inner.push(nc);
                    }
                    _ => { inner.push(nc); }
                }
            }
            if !inner.is_empty() {
                vars.push(format!("{{{inner}}}"));
            }
        } else if c == '}' && chars.peek() == Some(&'}') {
            // Escaped `}}` — skip
            chars.next();
        }
    }
    vars
}

/// Verify that all placeholders from `source` are present in `translated`.
/// Returns `None` if valid, or a warning string listing the missing ones.
pub fn check_placeholders(source: &FormattedMessage, translated: &FormattedMessage) -> Option<String> {
    let missing: Vec<String> = simple_placeholders(&source.text)
        .into_iter()
        .filter(|v| !translated.text.contains(v.as_str()))
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(format!("missing placeholders: {}", missing.join(", ")))
    }
}

pub fn make_client(cfg: &TranslateConfig) -> Client<OpenAIConfig> {
    Client::with_config(
        OpenAIConfig::new()
            .with_api_base(&cfg.url)
            .with_api_key(&cfg.api_key),
    )
}

pub async fn translate_text(
    text: &str,
    cfg: &TranslateConfig,
    client: &Client<OpenAIConfig>,
) -> Result<String, String> {
    // DO NOT MODIFY PROMPT, IT MUST BE EXACTLY LIKE THIS
    let prompt = format!(
        include_str!("translategemma.txt"),
        SOURCE_LANG = cfg.source_lang,
        SOURCE_CODE = cfg.source_code,
        TARGET_LANG = cfg.target_lang,
        TARGET_CODE = cfg.target_code,
        TEXT = text,
    );

    let response = client
        .chat()
        .create(CreateChatCompletionRequest {
            model: cfg.model.to_string(),
            messages: vec![ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(prompt),
                    name: None,
                },
            )],
            temperature: Some(0.5),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("API error: {e}"))?;

    Ok(response.choices[0]
        .message
        .content
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string())
}

pub async fn translate_file(
    source: &IntlFile,
    mut file: IntlFile,
    cfg: Arc<TranslateConfig>,
    pb: ProgressBar,
) -> IntlFile {
    let client = Arc::new(make_client(&cfg));

    for (key, msg) in file.messages_mut() {
        let translated_text = match translate_text(&msg.text, &cfg, &client).await {
            Ok(t) => t,
            Err(e) => {
                pb.println(format!("warn: failed to translate \"{key}\": {e}"));
                msg.text.to_string()
            }
        };
        pb.inc(1);

        // Verify placeholders survived; fall back to source text if not
        let translated_msg = FormattedMessage { text: translated_text, note: msg.note.clone() };
        let source_msg = source.messages().get(key).unwrap_or(msg);
        if let Some(warn) = check_placeholders(source_msg, &translated_msg) {
            pb.println(format!("warn: \"{key}\" {warn} — using source text"));
            msg.text = source_msg.text.clone();
        } else {
            msg.text = translated_msg.text;
        }
    }

    pb.finish_with_message(format!("{} ({}) done", cfg.target_lang, cfg.target_code));
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_placeholders_escaped() {
        // Escaped {{ }} should not be returned as placeholders
        assert!(simple_placeholders("{{not_a_placeholder}}").is_empty());
        assert!(simple_placeholders("Price: {{amount}} USD").is_empty());

        // Real placeholders alongside escaped braces
        let vars = simple_placeholders("{{escaped}} but {real} here");
        assert_eq!(vars, vec!["{real}"]);

        // Mixed: escaped and ICU plural
        let vars = simple_placeholders("{{nope}} {count, plural, one {# item} other {# items}}");
        assert_eq!(vars, vec!["{count, plural, one {# item} other {# items}}"]);
    }

    fn test_cfg() -> TranslateConfig {
        TranslateConfig {
            url: "http://localhost:11434/v1".to_string(),
            model: "translategemma:4b".to_string(),
            api_key: "ollama".to_string(),
            source_lang: "English".to_string(),
            source_code: "en".to_string(),
            target_lang: "German".to_string(),
            target_code: "de".to_string(),
        }
    }

    #[tokio::test]
    async fn test_formatting_preserved() {
        let cfg = test_cfg();
        let client = make_client(&cfg);

        let cases = [
            "Hello {name}, your renewal is due.",
            "Renewal for #{id}",
            "Location: {region}",
            "You have {count, plural, one {# message} other {# messages}}",
            "Your VM has {cpu} vCPU and {ram} RAM",
        ];

        for text in cases {
            let result = translate_text(text, &cfg, &client).await;
            assert!(result.is_ok(), "translation failed for: {text}");
            let translated = result.unwrap();
            println!("{text:?}\n  => {translated:?}\n");

            let source_msg = FormattedMessage { text: text.to_string(), note: None };
            let translated_msg = FormattedMessage { text: translated.clone(), note: None };
            assert!(
                check_placeholders(&source_msg, &translated_msg).is_none(),
                "placeholder check failed for {text:?}\n  got: {translated:?}"
            );
        }
    }
}

pub fn make_pb(
    cfg: &TranslateConfig,
    file: &IntlFile,
    mp: &MultiProgress,
) -> ProgressBar {
    let pb = ProgressBar::new(file.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>4}/{len:4} {msg}",
        )
        .unwrap()
        .progress_chars("##-"),
    );
    pb.set_message(format!("{} ({})", cfg.target_lang, cfg.target_code));
    mp.add(pb)
}
