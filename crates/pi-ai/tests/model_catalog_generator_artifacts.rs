//! The surviving embedded-artifact contract; old manifests/modes/rollback are retired.
use pi_ai::providers::models_data;
use pi_js_compat::js_json::{parse_lossless, to_value};
use std::path::PathBuf;

#[test]
fn native_include_move_preserves_every_embedded_catalog_value() {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/providers");
    let mut expected_ids = Vec::new();
    for entry in std::fs::read_dir(data).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if !name.ends_with(".json") || name == ".manifest.json" {
            continue;
        }
        let provider = name.strip_suffix(".json").unwrap();
        expected_ids.push(provider.to_owned());
        let json = std::fs::read_to_string(&path).unwrap();
        let expected = parse_lossless(&json).unwrap();
        let models = models_data::provider_models(provider).unwrap();
        assert_eq!(models.len(), expected.as_object().unwrap().len());
        for model in models {
            assert_eq!(
                to_value(&model).unwrap(),
                expected[model.id.as_str()],
                "{provider}/{}",
                model.id
            );
            assert_eq!(models_data::builtin_model(provider, &model.id), Some(model));
        }
    }
    expected_ids.sort();
    assert_eq!(models_data::builtin_provider_ids(), expected_ids);
    assert_eq!(models_data::provider_models("absent-provider"), None);
    assert_eq!(models_data::builtin_model("anthropic", "absent-model"), None);
}
