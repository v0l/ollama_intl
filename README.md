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

Usage: ollama_intl [OPTIONS] --target <TARGET>...

Options:
  -u, --url <URL>          Ollama base URL (must include /v1) [default: http://localhost:11434/v1]
  -m, --model <MODEL>      Model name [default: translategemma:27b]
      --source <SOURCE>    Source language as "Name:code" [default: English:en]
  -t, --target <TARGET>    Target language(s) as "Name:code", repeatable
  -o, --output-dir <DIR>   Directory to write output files; required when multiple targets are given
  -i, --input <INPUT>      Input file path, or "-" for stdin [default: -]
  -h, --help               Print help
```

### Translate to a single language

```sh
ollama_intl -i src/locales/en.json -o src/locales -t German:de
# writes src/locales/de.json
```

### Translate to multiple languages in parallel

All target languages are translated concurrently, each with its own progress bar.

```sh
ollama_intl \
  -i src/locales/en.json \
  -o src/locales \
  -t German:de \
  -t Spanish:es \
  -t French:fr \
  -t Portuguese:pt \
  -t Japanese:ja \
  -t Chinese:zh
```

### Translate a Rails YAML file

```sh
ollama_intl -i config/locales/en.yml -o config/locales -t French:fr
# writes config/locales/fr.yml
```

### Pipe from stdin

```sh
echo '{"greeting": "Hello"}' | ollama_intl -t German:de
```

### Remote Ollama instance

```sh
ollama_intl -u http://10.100.2.32:11434/v1 -i en.json -t Spanish:es -o .
```

## Prompt format

Uses the official TranslateGemma prompt format, with an added instruction to preserve ICU/FormatJS placeholders verbatim:

```
You are a professional {SOURCE_LANG} ({SOURCE_CODE}) to {TARGET_LANG} ({TARGET_CODE}) translator.
...
Any placeholders in curly braces such as {name}, {count}, {region} are code tokens and must be
copied exactly as-is, preserving their original capitalisation and spelling.
```

## Format details

### FormatJS / React-Intl

Only `defaultMessage` is translated. `description` is preserved verbatim as it is a developer hint, not user-visible text. Output keys are sorted alphabetically.

Input:
```json
{
  "2RFWLf": { "defaultMessage": "Speedtest" },
  "qq7WMq": { "defaultMessage": "All VPS come with 1x IPv4 and 1x IPv6 address.", "description": "pricing note" }
}
```

Output (`de.json`):
```json
{
  "2RFWLf": { "defaultMessage": "Geschwindigkeitstest" },
  "qq7WMq": { "defaultMessage": "Alle VPS werden mit je einer IPv4- und IPv6-Adresse geliefert.", "description": "pricing note" }
}
```

### Rails YAML

The top-level locale key is preserved in the output file regardless of target language — update it manually if required.

## License

MIT
