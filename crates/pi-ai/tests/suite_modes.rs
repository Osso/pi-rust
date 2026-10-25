// Process-local integration fixtures; process-sensitive fixtures keep standalone targets.

#[path = "support/oracle.rs"]
mod shared_oracle;

#[path = "api_model_headers.rs"]
pub mod api_model_headers;
#[path = "differential_image_models.rs"]
pub mod differential_image_models;
#[path = "differential_images_models.rs"]
pub mod differential_images_models;
#[path = "image_models.rs"]
pub mod image_models;
#[path = "images_models.rs"]
pub mod images_models;
#[path = "model_catalog_generator_artifacts.rs"]
pub mod model_catalog_generator_artifacts;
#[path = "model_catalog_generator_fetch.rs"]
pub mod model_catalog_generator_fetch;
#[path = "model_catalog_generator_metadata.rs"]
pub mod model_catalog_generator_metadata;
#[path = "model_catalog_generator_models_dev.rs"]
pub mod model_catalog_generator_models_dev;
#[path = "model_catalog_generator_native_http.rs"]
pub mod model_catalog_generator_native_http;
#[path = "model_types.rs"]
pub mod model_types;
#[path = "models.rs"]
pub mod models;
#[path = "models_contract.rs"]
pub mod models_contract;
#[path = "models_error_causes.rs"]
pub mod models_error_causes;
