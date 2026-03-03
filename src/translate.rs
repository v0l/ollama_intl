use std::collections::HashMap;

use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, CreateChatCompletionRequest,
    },
    Client,
};

use crate::types::{FormatJSMessage, IntlFile};

pub struct TranslateConfig<'a> {
    pub url: &'a str,
    pub model: &'a str,
    pub api_key: &'a str,
    pub source_lang: &'a str,
    pub source_code: &'a str,
    pub target_lang: &'a str,
    pub target_code: &'a str,
}

pub async fn translate_text(text: &str, cfg: &TranslateConfig<'_>) -> Result<String, String> {
    let prompt = format!(
        "You are a professional {sl} ({sc}) to {tl} ({tc}) translator. \
         Your goal is to accurately convey the meaning and nuances of the original {sl} text \
         while adhering to {tl} grammar, vocabulary, and cultural sensitivities. \
         Produce only the {tl} translation, without any additional explanations or commentary. \
         Please translate the following {sl} text into {tl}:\n\n{text}",
        sl = cfg.source_lang,
        sc = cfg.source_code,
        tl = cfg.target_lang,
        tc = cfg.target_code,
    );

    let client = Client::with_config(
        OpenAIConfig::new()
            .with_api_base(cfg.url)
            .with_api_key(cfg.api_key),
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

pub async fn translate_file(file: IntlFile, cfg: &TranslateConfig<'_>) -> IntlFile {
    async fn tr(key: &str, text: &str, cfg: &TranslateConfig<'_>) -> String {
        match translate_text(text, cfg).await {
            Ok(t) => t,
            Err(e) => {
                eprintln!("warn: failed to translate \"{key}\": {e}");
                text.to_string()
            }
        }
    }

    match file {
        IntlFile::Simple(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (key, value) in &map {
                out.insert(key.clone(), tr(key, value, cfg).await);
            }
            IntlFile::Simple(out)
        }
        IntlFile::FormatJS(map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (key, msg) in &map {
                out.insert(
                    key.clone(),
                    FormatJSMessage {
                        default_message: tr(key, &msg.default_message, cfg).await,
                        description: msg.description.clone(),
                    },
                );
            }
            IntlFile::FormatJS(out)
        }
        IntlFile::Rails(locale, map) => {
            let mut out = HashMap::with_capacity(map.len());
            for (key, value) in &map {
                out.insert(key.clone(), tr(key, value, cfg).await);
            }
            IntlFile::Rails(locale, out)
        }
    }
}
