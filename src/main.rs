mod parse;
mod translate;
mod types;

use clap::Parser;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use parse::{parse_input, serialise};
use translate::{translate_file, TranslateConfig};

const DEFAULT_URL: &str = "http://localhost:11434/v1";
const DEFAULT_MODEL: &str = "translategemma:27b";
const DEFAULT_API_KEY: &str = "ollama";

#[derive(Parser, Debug)]
#[command(name = "ollama_intl")]
#[command(about = "Translate React-Intl / i18n resource files using Ollama")]
struct Args {
    /// Ollama base URL (must include /v1)
    #[arg(short, long, default_value = DEFAULT_URL)]
    url: String,

    /// Model name
    #[arg(short, long, default_value = DEFAULT_MODEL)]
    model: String,

    /// Source language name (e.g. "English")
    #[arg(long, default_value = "English")]
    source_lang: String,

    /// Source language BCP-47 code (e.g. "en")
    #[arg(long, default_value = "en")]
    source_code: String,

    /// Target language name (e.g. "German")
    #[arg(short, long)]
    target_lang: String,

    /// Target language BCP-47 code (e.g. "de")
    #[arg(long)]
    target_code: String,

    /// Write output here instead of stdout; filename is <target_code>.<ext>
    #[arg(short, long)]
    output_dir: Option<PathBuf>,

    /// Input file path, or "-" for stdin
    #[arg(short, long, default_value = "-")]
    input: String,
}

fn read_input(path: &str) -> String {
    let mut buf = String::new();
    if path == "-" {
        io::stdin()
            .read_to_string(&mut buf)
            .expect("failed to read stdin");
    } else {
        fs::File::open(path)
            .and_then(|mut f| f.read_to_string(&mut buf))
            .unwrap_or_else(|e| {
                eprintln!("error: cannot read {path}: {e}");
                std::process::exit(1);
            });
    }
    buf
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let raw = read_input(&args.input);

    let parsed = parse_input(&raw, &args.input).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        std::process::exit(1);
    });

    let cfg = TranslateConfig {
        url: &args.url,
        model: &args.model,
        api_key: DEFAULT_API_KEY,
        source_lang: &args.source_lang,
        source_code: &args.source_code,
        target_lang: &args.target_lang,
        target_code: &args.target_code,
    };

    let translated = translate_file(parsed, &cfg).await;

    let output = serialise(&translated).unwrap_or_else(|e| {
        eprintln!("error: serialisation failed: {e}");
        std::process::exit(1);
    });

    match args.output_dir {
        Some(dir) => {
            let ext = if args.input.ends_with(".yaml") || args.input.ends_with(".yml") {
                "yml"
            } else {
                "json"
            };
            let path = dir.join(format!("{}.{}", args.target_code, ext));
            fs::write(&path, &output).unwrap_or_else(|e| {
                eprintln!("error: cannot write {}: {e}", path.display());
                std::process::exit(1);
            });
            println!("written: {}", path.display());
        }
        None => println!("{output}"),
    }
}
