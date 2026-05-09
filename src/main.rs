#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::as_conversions,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap,
        clippy::needless_collect,
        clippy::absolute_paths,
        clippy::if_then_some_else_none,
        clippy::doc_markdown,
        clippy::semicolon_outside_block,
        reason = "restriction/pedantic lints relaxed in test cfg — unwrap/expect/panic/casts and minor stylistic lints are idiomatic in tests"
    )
)]

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
