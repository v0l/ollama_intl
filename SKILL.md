---
name: ollama_intl
description: Translates i18n resource files (JSON, YAML) using Ollama and TranslateGemma. Supports Simple, FormatJS/React-Intl, and Rails formats. Detects format automatically and preserves ICU MessageFormat placeholders. Use when translating locale files, i18n resources, or when the user mentions translating JSON/YAML language files with Ollama.
license: MIT
compatibility: Requires a running Ollama instance with translategemma:27b pulled, or any OpenAI-compatible LLM endpoint. Requires Rust toolchain to build.
metadata:
  author: Kieran
  version: "0.1.0"
allowed-tools: Bash(cargo:*) Bash(ollama:*) Bash(ollama_intl:*) Read
---

# ollama-intl

Translates i18n resource files using [Ollama](https://ollama.com) and [TranslateGemma](https://ollama.com/library/translategemma). Supports three formats, auto-detected by content and file extension.

Install via `cargo install --git https://github.com/v0l/ollama_intl.git`. Written in Rust.

## Quick start

### Prerequisites

Make sure Ollama is running and the model is pulled:

```sh
ollama pull translategemma:27b
```

### Install

```sh
cargo install --git https://github.com/v0l/ollama_intl.git
```

This places `ollama_intl` in your Cargo bin directory (typically `~/.cargo/bin/`, which should be on `$PATH`).

To build locally instead:
```sh
cargo build --release
# binary: ./target/release/ollama_intl
```

### Usage

```
Usage: ollama_intl [OPTIONS] --target <TARGET>...

Options:
  -u, --url <URL>          Ollama base URL (must include /v1) [default: http://localhost:11434/v1]
  -m, --model <MODEL>      Model name [default: translategemma:27b]
      --source <SOURCE>    Source language as "Name:code" [default: English:en]
  -t, --target <TARGET>    Target language(s) as "Name:code", repeatable
  -o, --output-dir <DIR>   Directory to write output files; required
  -i, --input <INPUT>      Input file path, or "-" for stdin [default: -]
  -h, --help               Print help
```

### Examples

**Translate to a single language:**
```sh
ollama_intl -i src/locales/en.json -o src/locales -t German:de
# writes src/locales/de.json
```

**Translate to multiple languages in parallel (each with its own progress bar):**
```sh
ollama_intl \
  -i src/locales/en.json \
  -o src/locales \
  -t German:de -t Spanish:es -t French:fr \
  -t Portuguese:pt -t Japanese:ja -t Chinese:zh
```

**Translate a Rails YAML file:**
```sh
ollama_intl -i config/locales/en.yml -o config/locales -t French:fr
# writes config/locales/fr.yml
```

**Pipe from stdin:**
```sh
echo '{"greeting": "Hello"}' | ollama_intl -t German:de
```

**Use a remote Ollama instance:**
```sh
ollama_intl -u http://10.100.2.32:11434/v1 -i en.json -t Spanish:es -o .
```

**Retranslate everything, ignoring existing translations:**
```sh
ollama_intl -i en.json -o . -t German:de --force
```

## Supported formats

| Format | Description | Example |
|---|---|---|
| **Simple** | Flat key→string map | `{ "key": "value" }` / `key: value` |
| **FormatJS** | React-Intl compiled output | `{ "id": { "defaultMessage": "...", "description": "..." } }` |
| **Rails** | Ruby i18n YAML with locale wrapper | `en:\n  key: value` |

Format detection is automatic based on file extension (`.json` → JSON parsing, `.yaml`/`.yml` → YAML parsing) and content structure. FormatJS JSON is detected by the presence of `defaultMessage` keys.

## Features

### ICU MessageFormat support

The tool parses ICU MessageFormat (the standard behind react-intl/FormatJS) and translates each `Literal` segment individually, preserving argument names, plural keywords, `#`, `select` branches, and all non-literal tokens exactly as-is.

### Placeholder verification

After translation, all placeholders from the source are verified to still be present in the translated text. If any are missing, the source text is kept unchanged and a warning is printed.

### Generic plural detection

Strings using patterns like `{# {unit}s}` (where a runtime argument is mixed with a literal suffix inside a plural) are detected and warned about, since static translation cannot be accurate without knowing the runtime value.

### Incremental translation

By default, existing translation files are loaded and only missing keys are translated. Existing translations are preserved. Pass `--force` to retranslate everything.

### Key removal

Keys present in existing translations but no longer in the source file are removed from the output.

## Build and run during development

```sh
cargo run --release -- -i en.json -o output/ -t German:de
```

For just a debug build (faster compile, slower runtime):
```sh
cargo run -- -i en.json -o output/ -t German:de
```

## Running tests

```sh
cargo test              # unit tests only (no network)
cargo test -- --ignored  # integration tests requiring Ollama
```

## Project structure

```
ollama_intl/
├── SKILL.md                 # This file
├── Cargo.toml               # Rust dependencies
├── README.md                # Full documentation
└── src/
    ├── main.rs              # CLI arg parsing, orchestration, parallel spawning
    ├── parse.rs             # JSON/YAML parsing, format detection, serialisation
    ├── translate.rs         # LLM client, ICU parsing, translation, placeholder verification
    ├── types.rs             # IntlFile enum, FormattedMessage struct
    └── translategemma.txt   # LLM prompt template
```

## Modifying the code

- **CLI args**: edit the `Args` struct in `src/main.rs`
- **Adding a new format**: add a variant to `IntlFile` in `src/types.rs`, implement parsing in `src/parse.rs`, serialisation in `src/parse.rs::serialise`
- **Changing the LLM prompt**: edit `src/translategemma.txt`
- **Changing placeholder validation**: edit `check_placeholders` in `src/translate.rs`

## Edge cases and gotchas

- **Rails YAML**: The top-level locale key is preserved as-is in the output — update it manually if the target language code differs.
- **FormatJS descriptions**: The `description` field is preserved verbatim (not translated) since it's a developer hint, not user-visible text.
- **Escaped braces**: `{{` and `}}` are correctly treated as literal braces, not placeholders.
- **Time unit abbreviations**: The prompt contains special instructions to translate "hr", "min", "sec", etc. as time units not general vocabulary.
