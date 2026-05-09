use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use console::Term;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    CompleteEnv::with_factory(detail_cli::Cli::command).complete();

    match detail_cli::Cli::parse().run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _ = Term::stderr().write_line(&format!("Error: {err}"));
            ExitCode::FAILURE
        }
    }
}
