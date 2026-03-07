use clap::Parser;
use console::Term;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match detail_cli::Cli::parse().run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = Term::stderr().write_line(&format!("Error: {err}"));
            ExitCode::FAILURE
        }
    }
}
