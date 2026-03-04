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

use crate::types::IntlFile;

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
    mut file: IntlFile,
    cfg: Arc<TranslateConfig>,
    pb: ProgressBar,
) -> IntlFile {
    let client = Arc::new(make_client(&cfg));

    for (key, text) in file.messages_mut() {
        let translated_text = match translate_text(&text.text, &cfg, &client).await {
            Ok(t) => t,
            Err(e) => {
                pb.println(format!("warn: failed to translate \"{key}\": {e}"));
                text.text.to_string()
            }
        };
        pb.inc(1);
        text.text = translated_text;
    }

    pb.finish_with_message(format!("{} ({}) done", cfg.target_lang, cfg.target_code));
    file
}

#[cfg(test)]
mod tests {
    use super::*;

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

            // Check that simple {word} placeholders are preserved exactly (case-sensitive).
            // ICU plural blocks like {count, plural, ...} are checked as a whole string match.
            let simple_var_re = |s: &str| -> Vec<String> {
                let mut vars = vec![];
                let mut chars = s.chars().peekable();
                while let Some(c) = chars.next() {
                    if c == '{' {
                        let inner: String = chars.by_ref().take_while(|&c| c != '}').collect();
                        // Only simple identifiers — no spaces or commas
                        if !inner.is_empty() && inner.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            vars.push(inner);
                        }
                    }
                }
                vars
            };

            for var in simple_var_re(text) {
                assert!(
                    translated.contains(&format!("{{{var}}}")),
                    "placeholder {{{var}}} was lost in translation of {text:?}\n  got: {translated:?}"
                );
            }
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
