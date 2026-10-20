#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::image_catalog_generator::{generate_image_models_file, parse_openrouter_image_models};
use pi_js_compat::js_json::{parse_lossless, stringify_lossless};
use serde_json::json;

#[test]
fn generated_artifact_matches_source_prototype_named_model() {
    let fixtures = [
        r#"{"data":[{"id":"__proto__","name":"prototype","architecture":{"output_modalities":["image"]}}]}"#,
        r#"{"data":[{"id":"nested","name":{"__proto__":{"lost":true},"keep":[{"__proto__":1,"visible":2}]},"architecture":{"output_modalities":["image"]}}]}"#,
    ];
    for (case, payload) in fixtures.into_iter().enumerate() {
        let mut ts = oracle::TsOracle::spawn(
            "image-catalog-generator-oracle.ts",
            "packages/ai/scripts/generate-image-models.ts",
        );
        let expected = ts.request(&json!({"operation":"workflow","payload":payload}));
        assert_eq!(expected["status"], 0, "{expected}");
        if case == 1 {
            // Approved wrong-field rejection: source's nested prototype projection is now invalid input.
            let error = parse_openrouter_image_models(&parse_lossless(payload).unwrap(), true).unwrap_err();
            assert_eq!(error.name, "TypeError");
            assert_eq!(error.message, "Invalid OpenRouter image model name: expected string");
            assert_eq!(
                expected["catalog"].as_str().unwrap(),
                r#"{"openrouter":{"nested":{"id":"nested","name":{"keep":[{"visible":2}]},"api":"openrouter-images","provider":"openrouter","baseUrl":"https://openrouter.ai/api/v1","input":["text"],"output":["image"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}}}}"#
            );
            continue;
        }
        let artifact = generate_image_models_file(
            &parse_openrouter_image_models(&parse_lossless(payload).unwrap(), true).unwrap(),
        )
        .unwrap();
        assert_eq!(
            stringify_lossless(&parse_lossless(&artifact).unwrap()),
            expected["catalog"].as_str().unwrap()
        );
    }
}
