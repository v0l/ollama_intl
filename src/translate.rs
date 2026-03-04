use std::sync::Arc;

use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent, CreateChatCompletionRequest,
    },
    Client,
};
use formatjs_icu_messageformat_parser::{
    parser::{Parser, ParserOptions},
    print_ast,
    types::MessageFormatElement,
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

/// Returns true if any plural or select branch mixes a runtime `Argument` with
/// a non-whitespace `Literal` sibling — the `{# {unit}s}` pattern that cannot
/// be accurately translated without knowing the runtime value of the argument.
pub fn has_generic_plural(elements: &[MessageFormatElement]) -> bool {
    for el in elements {
        match el {
            MessageFormatElement::Plural(plural) => {
                for opt in plural.options.values() {
                    let has_arg = opt.value.iter().any(|e| matches!(e, MessageFormatElement::Argument(_)));
                    let has_text = opt.value.iter().any(|e| {
                        matches!(e, MessageFormatElement::Literal(l) if !l.value.trim().is_empty())
                    });
                    if has_arg && has_text {
                        return true;
                    }
                    if has_generic_plural(&opt.value) {
                        return true;
                    }
                }
            }
            MessageFormatElement::Select(select) => {
                for opt in select.options.values() {
                    if has_generic_plural(&opt.value) {
                        return true;
                    }
                }
            }
            MessageFormatElement::Tag(tag) => {
                if has_generic_plural(&tag.children) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Collect all `Literal` values from an ICU AST, recursing into plural/select branches.
/// Returns them in depth-first order.
fn collect_literals(elements: &[MessageFormatElement]) -> Vec<String> {
    let mut out = vec![];
    for el in elements {
        match el {
            MessageFormatElement::Literal(lit) => {
                out.push(lit.value.clone());
            }
            MessageFormatElement::Plural(plural) => {
                for opt in plural.options.values() {
                    out.extend(collect_literals(&opt.value));
                }
            }
            MessageFormatElement::Select(select) => {
                for opt in select.options.values() {
                    out.extend(collect_literals(&opt.value));
                }
            }
            MessageFormatElement::Tag(tag) => {
                out.extend(collect_literals(&tag.children));
            }
            _ => {}
        }
    }
    out
}

/// Write translated literal values back into an ICU AST in the same depth-first order.
fn apply_literals(elements: &mut Vec<MessageFormatElement>, translated: &[String], idx: &mut usize) {
    for el in elements.iter_mut() {
        match el {
            MessageFormatElement::Literal(lit) => {
                if let Some(t) = translated.get(*idx) {
                    lit.value = t.clone();
                }
                *idx += 1;
            }
            MessageFormatElement::Plural(plural) => {
                for opt in plural.options.values_mut() {
                    apply_literals(&mut opt.value, translated, idx);
                }
            }
            MessageFormatElement::Select(select) => {
                for opt in select.options.values_mut() {
                    apply_literals(&mut opt.value, translated, idx);
                }
            }
            MessageFormatElement::Tag(tag) => {
                apply_literals(&mut tag.children, translated, idx);
            }
            _ => {}
        }
    }
}

/// Send a single string to the LLM and return the translated result.
async fn call_llm(
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

/// Translate a message string.
///
/// If the text parses as valid ICU MessageFormat, each `Literal` segment
/// (including those nested inside `plural`/`select` branches) is translated
/// individually, and the AST is reprinted so all non-literal tokens
/// (argument names, plural keywords, `#`, etc.) are preserved exactly.
///
/// Falls back to translating the raw string when the text contains no ICU
/// expressions (i.e. no `{` characters).
pub async fn translate_text(
    text: &str,
    cfg: &TranslateConfig,
    client: &Client<OpenAIConfig>,
) -> Result<String, String> {
    // Only try ICU parsing when there are braces; plain strings go straight to LLM.
    if text.contains('{') {
        let parser = Parser::new(text, ParserOptions::default());
        if let Ok(mut ast) = parser.parse() {
            let literals = collect_literals(&ast);
            // Translate each non-whitespace-only literal segment
            let mut translated_literals = Vec::with_capacity(literals.len());
            for lit in &literals {
                let trimmed = lit.trim();
                if trimmed.is_empty() {
                    translated_literals.push(lit.clone());
                } else {
                    let t = call_llm(trimmed, cfg, client).await?;
                    // Preserve leading/trailing whitespace from the original
                    let leading: String = lit.chars().take_while(|c| c.is_whitespace()).collect();
                    let trailing: String = lit.chars().rev().take_while(|c| c.is_whitespace()).collect::<String>().chars().rev().collect();
                    translated_literals.push(format!("{leading}{t}{trailing}"));
                }
            }
            let mut idx = 0;
            apply_literals(&mut ast, &translated_literals, &mut idx);
            return Ok(print_ast(&ast));
        }
    }

    // Plain string — translate directly
    call_llm(text, cfg, client).await
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

/// Collect all argument names from an ICU AST (Argument, Plural, Select `value` fields).
fn collect_arg_names(elements: &[MessageFormatElement]) -> Vec<String> {
    let mut out = vec![];
    for el in elements {
        match el {
            MessageFormatElement::Argument(a) => out.push(a.value.clone()),
            MessageFormatElement::Plural(p) => {
                out.push(p.value.clone());
                for opt in p.options.values() {
                    out.extend(collect_arg_names(&opt.value));
                }
            }
            MessageFormatElement::Select(s) => {
                out.push(s.value.clone());
                for opt in s.options.values() {
                    out.extend(collect_arg_names(&opt.value));
                }
            }
            MessageFormatElement::Number(n) => out.push(n.value.clone()),
            MessageFormatElement::Date(d) => out.push(d.value.clone()),
            MessageFormatElement::Time(t) => out.push(t.value.clone()),
            MessageFormatElement::Tag(tag) => {
                out.push(tag.value.clone());
                out.extend(collect_arg_names(&tag.children));
            }
            _ => {}
        }
    }
    out
}

/// Verify that all placeholders from `source` are present in `translated`.
/// Uses the ICU AST when possible (whitespace-independent), falls back to
/// substring matching for non-ICU strings.
/// Returns `None` if valid, or a warning string listing the missing ones.
pub fn check_placeholders(source: &FormattedMessage, translated: &FormattedMessage) -> Option<String> {
    // Try AST-based comparison first
    if source.text.contains('{') {
        if let (Ok(src_ast), Ok(tgt_ast)) = (
            Parser::new(&source.text, ParserOptions::default()).parse(),
            Parser::new(&translated.text, ParserOptions::default()).parse(),
        ) {
            let src_args = collect_arg_names(&src_ast);
            let tgt_args = collect_arg_names(&tgt_ast);
            let missing: Vec<String> = src_args
                .into_iter()
                .filter(|a| !tgt_args.contains(a))
                .map(|a| format!("{{{a}}}"))
                .collect();
            return if missing.is_empty() {
                None
            } else {
                Some(format!("missing placeholders: {}", missing.join(", ")))
            };
        }
    }

    // Fallback: raw substring check
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

pub async fn translate_file(
    source: &IntlFile,
    mut file: IntlFile,
    cfg: Arc<TranslateConfig>,
    pb: ProgressBar,
) -> IntlFile {
    let client = Arc::new(make_client(&cfg));

    for (key, msg) in file.messages_mut() {
        // Warn if the message uses a generic plural pattern like `{# {unit}s}` —
        // static translation cannot be accurate without knowing the runtime value.
        if msg.text.contains('{') {
            if let Ok(ast) = Parser::new(&msg.text, ParserOptions::default()).parse() {
                if has_generic_plural(&ast) {
                    pb.println(format!(
                        "warn: \"{key}\" contains a generic plural with a runtime argument — \
                         accurate translation is not possible; consider using a select on the unit name instead"
                    ));
                    pb.inc(1);
                    continue;
                }
            }
        }

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
    fn test_has_generic_plural() {
        let parse = |s| Parser::new(s, ParserOptions::default()).parse().unwrap();

        // The problematic pattern: runtime arg mixed with literal suffix inside plural
        assert!(has_generic_plural(&parse(
            "{n, plural, one {# {unit}} other {# {unit}s}}"
        )));

        // Normal plural with only literals — fine
        assert!(!has_generic_plural(&parse(
            "{n, plural, one {# minute} other {# minutes}}"
        )));

        // Simple argument — fine
        assert!(!has_generic_plural(&parse("Hello {name}")));

        // Plural with only a Pound and Argument but no literal text — fine
        assert!(!has_generic_plural(&parse(
            "{n, plural, one {# {unit}} other {# {unit}}}"
        )));
    }

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

    #[test]
    fn test_collect_literals_simple() {
        let parser = Parser::new("Hello world", ParserOptions::default());
        let ast = parser.parse().unwrap();
        let lits = collect_literals(&ast);
        assert_eq!(lits, vec!["Hello world"]);
    }

    #[test]
    fn test_collect_literals_plural() {
        let text = "You have {count, plural, one {# message} other {# messages}}";
        let parser = Parser::new(text, ParserOptions::default());
        let ast = parser.parse().unwrap();
        let lits = collect_literals(&ast);
        // "You have " is the outer literal; then " message" and " messages" are inside the plural
        assert!(lits.iter().any(|l| l.contains("message")));
        assert!(lits.iter().any(|l| l.contains("messages")));
    }

    #[test]
    fn test_apply_literals_roundtrip() {
        let text = "You have {count, plural, one {# message} other {# messages}}";
        let parser = Parser::new(text, ParserOptions::default());
        let mut ast = parser.parse().unwrap();
        let lits = collect_literals(&ast);
        // Apply same values back — the reprinted string may differ in whitespace
        // but argument names and literal content must be preserved.
        let mut idx = 0;
        apply_literals(&mut ast, &lits, &mut idx);
        let reprinted = print_ast(&ast);
        // Argument name must be present
        assert!(reprinted.contains("count"), "arg name lost: {reprinted}");
        // Literal words must be present
        assert!(reprinted.contains("message"), "literal lost: {reprinted}");
        assert!(reprinted.contains("messages"), "literal lost: {reprinted}");
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

    /// Verify that time-unit words inside ICU plural branches are translated
    /// correctly (as time units, not general vocabulary) and that the argument
    /// name and plural keywords are preserved in the output.
    async fn assert_plural_time_units(cfg: &TranslateConfig, cases: &[(&str, &str, &[&str])]) {
        let client = make_client(cfg);
        for (text, arg_name, expected_substrings) in cases {
            let result = translate_text(text, cfg, &client).await;
            assert!(result.is_ok(), "translation failed for: {text}");
            let translated = result.unwrap();
            println!("{text:?}\n  => {translated:?}\n");

            // Argument name must be preserved
            assert!(
                translated.contains(arg_name),
                "arg name {arg_name:?} lost in: {translated:?}"
            );

            // At least one of the expected substrings must appear (case-insensitive)
            let lower = translated.to_lowercase();
            let found = expected_substrings
                .iter()
                .any(|s| lower.contains(&s.to_lowercase()));
            assert!(
                found,
                "none of {expected_substrings:?} found in translation of {text:?}: got {translated:?}"
            );

            // Placeholder check: arg name must not have been dropped
            let source_msg = FormattedMessage { text: text.to_string(), note: None };
            let translated_msg = FormattedMessage { text: translated.clone(), note: None };
            assert!(
                check_placeholders(&source_msg, &translated_msg).is_none(),
                "placeholder check failed for {text:?}\n  got: {translated:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_plural_time_units_de() {
        assert_plural_time_units(
            &test_cfg(),
            &[
                ("{n, plural, one {# sec} other {# secs}}", "n", &["Sek", "sek"]),
                ("{n, plural, one {# min} other {# mins}}", "n", &["Min", "min"]),
                ("{n, plural, one {# hr} other {# hrs}}", "n", &["Std", "Stun", "std"]),
                ("{n, plural, one {# day} other {# days}}", "n", &["Tag", "tag"]),
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn test_plural_time_units_zh() {
        let cfg = TranslateConfig {
            url: "http://localhost:11434/v1".to_string(),
            model: "translategemma:4b".to_string(),
            api_key: "ollama".to_string(),
            source_lang: "English".to_string(),
            source_code: "en".to_string(),
            target_lang: "Chinese".to_string(),
            target_code: "zh".to_string(),
        };
        assert_plural_time_units(
            &cfg,
            &[
                ("{n, plural, one {# sec} other {# secs}}", "n", &["秒"]),
                ("{n, plural, one {# min} other {# mins}}", "n", &["分"]),
                ("{n, plural, one {# hr} other {# hrs}}", "n", &["时", "小时", "時"]),
                ("{n, plural, one {# day} other {# days}}", "n", &["天", "日"]),
            ],
        )
        .await;
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
