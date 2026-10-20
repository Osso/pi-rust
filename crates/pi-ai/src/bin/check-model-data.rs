//! Offline checker for the Rust catalog, run from the repository root.
use std::process::ExitCode;

fn main() -> ExitCode {
    let validation = std::env::current_dir()
        .map_err(|error| error.to_string())
        .and_then(|root| pi_ai::model_data_tools::validate_native_model_data(&root).map_err(|error| error.to_string()));
    match validation {
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
