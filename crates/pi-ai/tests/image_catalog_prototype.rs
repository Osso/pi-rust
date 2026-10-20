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
    for payload in fixtures {
        let mut ts = oracle::TsOracle::spawn(
            "image-catalog-generator-oracle.ts",
            "packages/ai/scripts/generate-image-models.ts",
        );
        let expected = ts.request(&json!({"operation":"workflow","payload":payload}));
        assert_eq!(expected["status"], 0, "{expected}");
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
