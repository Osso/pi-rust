//! Offline checker for the Rust catalog at its compiled source location.
use std::path::Path;
use std::process::ExitCode;

use pi_ai::model_data_tools::{native_model_data_structure, validate_model_data_directory};

fn main() -> ExitCode {
    let data_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/providers");
    let validation = validate_model_data_directory(&native_model_data_structure(), &data_dir);
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
