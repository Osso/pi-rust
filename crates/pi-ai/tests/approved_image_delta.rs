use crate::shared_oracle as oracle;

use pi_ai::image_models::{get_image_model, get_image_models, get_image_providers};
use pi_ai::types::ImagesModel;
use serde_json::{Value, json};

const PROVIDER: &str = "openrouter";
const MODEL: &str = "google/gemini-2.5-flash-image";

fn unchanged_fields(model: &ImagesModel, original: &ImagesModel) -> Value {
    json!({"name":model.name == original.name,"input":model.input == original.input,
        "output":model.output == original.output,"cost":model.cost == original.cost})
}

fn native_mutation(getter: &str) -> Value {
    // Registry getters expose immutable borrows; callers need owned clones to mutate.
    let original: &'static ImagesModel = get_image_model(PROVIDER, MODEL).unwrap();
    let mut owned = if getter == "single" {
        original.clone()
    } else {
        get_image_models(PROVIDER)
            .iter()
            .find(|model| model.id == MODEL)
            .unwrap()
            .clone()
    };
    owned.name = "caller-owned name".into();
    owned.input.clear();
    owned.output.clear();
    owned.cost.rates.input = 999.0;
    assert_ne!(&owned, original);
    let mut providers = get_image_providers();
    let original_providers = providers.clone();
    providers.clear();
    let mut models = get_image_models(PROVIDER).to_vec();
    let original_count = models.len();
    models.clear();
    json!({"mutation":{
        "single":unchanged_fields(get_image_model(PROVIDER, MODEL).unwrap(), original),
        "list":unchanged_fields(get_image_models(PROVIDER).iter().find(|model| model.id == MODEL).unwrap(), original)},
        "providersIndependent":get_image_providers() == original_providers,
        "modelsIndependent":get_image_models(PROVIDER).len() == original_count})
}

fn source_mutation(delta: bool, getter: &str) -> Value {
    let mut ts = if delta {
        oracle::TsOracle::spawn_with_environment(
            "approved-images-oracle.mjs",
            "packages/ai/src/image-models.ts",
            &[("PI_APPROVED_DELTA", "1".into())],
        )
    } else {
        oracle::TsOracle::spawn("approved-images-oracle.mjs", "packages/ai/src/image-models.ts")
    };
    ts.request(&json!({"getter":getter}))
}

fn expected(unchanged: bool) -> Value {
    let fields = json!({"name":unchanged,"input":unchanged,"output":unchanged,"cost":unchanged});
    json!({"mutation":{"single":fields,"list":fields},"providersIndependent":true,"modelsIndependent":true})
}

#[test]
fn immutable_getters_match_delta_copy_isolation_for_scalar_nested_and_list_mutations() {
    for getter in ["single", "list"] {
        assert_eq!(source_mutation(true, getter), expected(true), "{getter}");
        assert_eq!(native_mutation(getter), expected(true), "{getter}");
    }
}

#[test]
fn frozen_image_getters_have_exact_approved_nested_mutation_discrepancy() {
    for getter in ["single", "list"] {
        assert_eq!(source_mutation(false, getter), expected(false), "{getter}");
        assert_eq!(native_mutation(getter), expected(true), "{getter}");
    }
}
