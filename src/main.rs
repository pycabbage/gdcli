use clap::Parser;
use gdcli::Cli;
use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match gdcli::run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {}", gdcli::message(&error));
            ExitCode::FAILURE
        }
    }
}
