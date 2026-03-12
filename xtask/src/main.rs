use clap::{Parser, Subcommand};
use std::process;

const OPENAPI_URL: &str = "https://api.detail.dev/public/v1/openapi.json";
const OPENAPI_PATH: &str = "openapi.json";
const HELP_PATH: &str = "docs/HELP.md";

#[derive(Parser)]
#[command(name = "xtask", about = "Dev tasks for detail-cli")]
struct Xtask {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run all vendored-artifact checks
    Check,
    /// Check docs/HELP.md is up to date
    CheckHelp,
    /// Check openapi.json matches upstream
    CheckOpenapi,
    /// Print generated HELP.md to stdout
    GenerateHelp,
    /// Fetch and write openapi.json from upstream
    GenerateOpenapi,
}

fn main() {
    let cli = Xtask::parse();

    let result = match cli.command {
        Command::Check => check_all(),
        Command::CheckHelp => check_help(),
        Command::CheckOpenapi => check_openapi(),
        Command::GenerateHelp => generate_help(),
        Command::GenerateOpenapi => generate_openapi(),
    };

    if let Err(err) = result {
        eprintln!("Error: {err}");
        process::exit(1);
    }
}

fn generate_help() -> Result<(), String> {
    let help = clap_markdown::help_markdown::<detail_cli::Cli>();
    print!("{help}");
    Ok(())
}

fn check_help() -> Result<(), String> {
    let expected = clap_markdown::help_markdown::<detail_cli::Cli>();
    let current = std::fs::read_to_string(HELP_PATH)
        .map_err(|e| format!("Failed to read {HELP_PATH}: {e}"))?;

    if current.trim() == expected.trim() {
        eprintln!("{HELP_PATH} is up to date.");
        Ok(())
    } else {
        Err(format!(
            "{HELP_PATH} is out of date. Run `cargo xtask generate-help > {HELP_PATH}` to regenerate it."
        ))
    }
}

#[tokio::main]
async fn fetch_openapi() -> Result<serde_json::Value, String> {
    let resp = reqwest::get(OPENAPI_URL)
        .await
        .map_err(|e| format!("Failed to fetch {OPENAPI_URL}: {e}"))?;
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse upstream JSON: {e}"))
}

const OPENAPI_COMMENT: &str = "Generated file — do not edit directly. Source of truth: \
    apps/backend/src/app/routes/public/v1/openapi-spec.ts in the detail repo. \
    Run `cargo xtask generate-openapi` to regenerate.";

fn with_comment(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "$comment".to_string(),
            serde_json::Value::String(OPENAPI_COMMENT.to_string()),
        );
    }
    value
}

fn generate_openapi() -> Result<(), String> {
    let upstream = fetch_openapi()?;
    let pretty = serde_json::to_string_pretty(&with_comment(upstream))
        .map_err(|e| format!("Failed to format JSON: {e}"))?;
    std::fs::write(OPENAPI_PATH, format!("{pretty}\n"))
        .map_err(|e| format!("Failed to write {OPENAPI_PATH}: {e}"))?;
    eprintln!("Wrote {OPENAPI_PATH}");
    Ok(())
}

fn check_openapi() -> Result<(), String> {
    let upstream = fetch_openapi()?;
    let local_bytes = std::fs::read_to_string(OPENAPI_PATH)
        .map_err(|e| format!("Failed to read {OPENAPI_PATH}: {e}"))?;
    let mut local: serde_json::Value = serde_json::from_str(&local_bytes)
        .map_err(|e| format!("Failed to parse local {OPENAPI_PATH}: {e}"))?;
    if let Some(obj) = local.as_object_mut() {
        obj.remove("$comment");
    }

    if upstream == local {
        eprintln!("{OPENAPI_PATH} matches upstream.");
        Ok(())
    } else {
        Err(format!(
            "{OPENAPI_PATH} does not match upstream. Run `cargo xtask generate-openapi` to update it."
        ))
    }
}

fn check_all() -> Result<(), String> {
    eprintln!("Checking vendored artifacts...");
    let mut failed = false;

    if let Err(err) = check_openapi() {
        eprintln!("{err}");
        failed = true;
    }

    if let Err(err) = check_help() {
        eprintln!("{err}");
        failed = true;
    }

    if failed {
        Err("One or more checks failed.".to_string())
    } else {
        Ok(())
    }
}
