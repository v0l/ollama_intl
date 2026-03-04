mod parse;
mod translate;
mod types;

use std::sync::Arc;

use clap::Parser;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use indicatif::MultiProgress;
use parse::{parse_input, serialise};
use tokio::signal;
use tokio::task::JoinSet;
use translate::{make_pb, translate_file, TranslateConfig};

const DEFAULT_URL: &str = "http://localhost:11434/v1";
const DEFAULT_MODEL: &str = "translategemma:27b";
const DEFAULT_API_KEY: &str = "ollama";

/// A target language specified as "Name:code", e.g. "German:de"
#[derive(Debug, Clone)]
struct Target {
    lang: String,
    code: String,
}

impl std::str::FromStr for Target {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (lang, code) = s
            .split_once(':')
            .ok_or_else(|| format!("expected \"Language:code\", got \"{s}\""))?;
        Ok(Target {
            lang: lang.to_string(),
            code: code.to_string(),
        })
    }
}

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

    /// Source language name and BCP-47 code (e.g. "English:en")
    #[arg(long, default_value = "English:en")]
    source: Target,

    /// Target language(s) as "Name:code", repeatable (e.g. -t German:de -t French:fr)
    #[arg(short, long = "target", required = true)]
    targets: Vec<Target>,

    /// Directory to write output files; required when multiple targets are given
    #[arg(short, long)]
    output_dir: PathBuf,

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

    let ext = if args.input.ends_with(".yaml") || args.input.ends_with(".yml") {
        "yml"
    } else {
        "json"
    };

    let mp = MultiProgress::new();
    let mut set: JoinSet<(Target, _)> = JoinSet::new();

    for target in &args.targets {
        let cfg = Arc::new(TranslateConfig {
            url: args.url.clone(),
            model: args.model.clone(),
            api_key: DEFAULT_API_KEY.to_string(),
            source_lang: args.source.lang.clone(),
            source_code: args.source.code.clone(),
            target_lang: target.lang.clone(),
            target_code: target.code.clone(),
        });

        let pb = make_pb(&cfg, &parsed, &mp);
        let file = parsed.clone();
        let target = target.clone();

        set.spawn(async move {
            let translated = translate_file(file, cfg, pb).await;
            (target, translated)
        });
    }
    mp.println("Starting..").unwrap();

    let mut results: Vec<(Target, _)> = Vec::new();
    let completed = async {
        while let Some(res) = set.join_next().await {
            results.push(res.expect("translation task panicked"));
        }
    };

    tokio::select! {
        _ = completed => {}
        _ = signal::ctrl_c() => {
            let _ = mp.clear();
            eprintln!("interrupted");
            std::process::exit(130);
        }
    }
    let _ = mp.clear();

    for (target, translated) in results {
        let output = serialise(&translated).unwrap_or_else(|e| {
            eprintln!("error: serialisation failed for {}: {e}", target.code);
            std::process::exit(1);
        });

        let path = args.output_dir.join(format!("{}.{}", target.code, ext));
        fs::write(&path, &output).unwrap_or_else(|e| {
            eprintln!("error: cannot write {}: {e}", path.display());
            std::process::exit(1);
        });
        println!("written: {}", path.display());
    }
}
