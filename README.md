# ollama_intl

Translates i18n resource files using [Ollama](https://ollama.com) and [TranslateGemma](https://ollama.com/library/translategemma).

Supports three common formats, auto-detected by content and file extension:

| Format | Description | Example |
|---|---|---|
| **Simple** | Flat key→string map | `{ "key": "value" }` / `key: value` |
| **FormatJS** | React-Intl compiled output | `{ "id": { "defaultMessage": "...", "description": "..." } }` |
| **Rails** | Ruby i18n YAML with locale wrapper | `en:\n  key: value` |

## Requirements

- [Ollama](https://ollama.com) running with `translategemma:27b` pulled
- Rust toolchain (to build)

```sh
ollama pull translategemma:27b
```

## Build

```sh
cargo build --release
```

## Usage

```
Translate React-Intl / i18n resource files using Ollama

Usage: ollama_intl [OPTIONS] --target-lang <TARGET_LANG> --target-code <TARGET_CODE>

Options:
  -u, --url <URL>                  Ollama base URL (must include /v1) [default: http://localhost:11434/v1]
  -m, --model <MODEL>              Model name [default: translategemma:27b]
      --source-lang <SOURCE_LANG>  Source language name (e.g. "English") [default: English]
      --source-code <SOURCE_CODE>  Source language BCP-47 code (e.g. "en") [default: en]
  -t, --target-lang <TARGET_LANG>  Target language name (e.g. "German")
      --target-code <TARGET_CODE>  Target language BCP-47 code (e.g. "de")
  -o, --output-dir <OUTPUT_DIR>    Write output here instead of stdout; filename is <target_code>.<ext>
  -i, --input <INPUT>              Input file path, or "-" for stdin [default: -]
  -h, --help                       Print help
```

### Translate a FormatJS JSON file to German

```sh
ollama_intl \
  -i src/locales/en.json \
  -o src/locales \
  --target-lang German \
  --target-code de
# writes src/locales/de.json
```

### Translate a Rails YAML file to French

```sh
ollama_intl \
  -i config/locales/en.yml \
  -o config/locales \
  --target-lang French \
  --target-code fr
# writes config/locales/fr.yml
```

### Pipe from stdin

```sh
echo '{"greeting": "Hello"}' | ollama_intl --target-lang German --target-code de
```

### Remote Ollama instance

```sh
ollama_intl \
  -u http://10.100.2.32:11434/v1 \
  -i en.json \
  --target-lang Spanish \
  --target-code es
```

## Prompt format

Uses the official TranslateGemma prompt format:

```
You are a professional {SOURCE_LANG} ({SOURCE_CODE}) to {TARGET_LANG} ({TARGET_CODE}) translator.
Your goal is to accurately convey the meaning and nuances of the original {SOURCE_LANG} text while
adhering to {TARGET_LANG} grammar, vocabulary, and cultural sensitivities.
Produce only the {TARGET_LANG} translation, without any additional explanations or commentary.
Please translate the following {SOURCE_LANG} text into {TARGET_LANG}:

{TEXT}
```

## Format details

### FormatJS / React-Intl

Only `defaultMessage` is translated. `description` is preserved verbatim as it is a developer hint, not user-visible text.

Input:
```json
{
  "2RFWLf": { "defaultMessage": "Speedtest" },
  "qq7WMq": { "defaultMessage": "All VPS come with 1x IPv4 and 1x IPv6 address and unmetered traffic, all prices are excluding taxes.", "description": "pricing note" }
}
```

Output (`de.json`):
```json
{
  "2RFWLf": { "defaultMessage": "Geschwindigkeitstest" },
  "qq7WMq": { "defaultMessage": "Alle VPS werden mit jeweils einer IPv4- und einer IPv6-Adresse sowie unbegrenztem Datenvolumen geliefert...", "description": "pricing note" }
}
```

### Rails YAML

The top-level locale key is preserved in the output file regardless of target language — update it manually if required.

## License

MIT
