//! Generated image-model catalog.
//!
//! Values come from `packages/ai/src/image-models.generated.ts` at the pinned
//! revision in `docs/porting/source-lock.json`. The checked-in
//! `data/image-models.json` is the TypeScript object serialized with
//! `JSON.stringify(IMAGE_MODELS, null, 2)`, so provider and model insertion
//! order is preserved. See `docs/porting/traceability/image-models.md` for the
//! regeneration command.

use std::sync::LazyLock;

use serde_json::Value;

use crate::types::ImagesModel;

static IMAGE_MODELS: LazyLock<Vec<(String, Vec<ImagesModel>)>> = LazyLock::new(load_image_models);

fn load_image_models() -> Vec<(String, Vec<ImagesModel>)> {
    let root: serde_json::Map<String, Value> =
        serde_json::from_str(include_str!("../data/image-models.json"))
            .expect("embedded image model catalog must be valid JSON");
    root.into_iter()
        .map(|(provider, models)| {
            let models = models
                .as_object()
                .expect("image model provider entry must be an object")
                .values()
                .map(|model| {
                    serde_json::from_value(model.clone()).expect("image model entry must match ImagesModel")
                })
                .collect();
            (provider, models)
        })
        .collect()
}

/// `IMAGE_MODELS` groups in generated order.
pub fn image_model_groups() -> &'static [(String, Vec<ImagesModel>)] {
    &IMAGE_MODELS
}
