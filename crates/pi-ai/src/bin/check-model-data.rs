//! Offline checker for an explicitly supplied AI package directory.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(root) = args.next().filter(|_| args.len() == 0) else {
        eprintln!("Usage: check-model-data <ai-package-root>");
        return ExitCode::from(2);
    };
    match pi_ai::model_data_tools::validate_generated_model_data(&PathBuf::from(root)) {
        Ok(()) => {
            println!("Generated model data is valid.");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            eprintln!("\nModel data is missing or stale. Run `npm run hydrate:model-data` from the repository root.");
            ExitCode::FAILURE
        }
    }
}
