#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::model_catalog_generator::read_generator_options;
use serde_json::json;

#[test]
fn source_cli_option_precedence_and_validation_match() {
    let mut source = oracle::TsOracle::spawn(
        "model-catalog-generator-oracle.ts",
        "packages/ai/scripts/generate-models.ts",
    );
    for args in [
        vec![],
        vec!["--strict", "--strict", "--pretty", "--data-only"],
        vec!["--json-only", "--json-output", "/tmp/native-model-test"],
        vec!["--json-output", "/tmp/a", "--json-output", "/tmp/b"],
        vec!["--json-only"],
        vec!["--json-output"],
        vec!["--json-output", ""],
        vec!["--data-only", "--json-output", "/tmp/a"],
        vec!["--data-only", "--json-only"],
        vec!["--wat"],
        vec!["--json-output", "--pretty"],
        vec!["--help"],
    ] {
        let expected = source.request(&json!({"operation":"readGeneratorOptions","args":[args]}));
        let actual = read_generator_options(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>());
        match actual {
            Ok(options) => assert_eq!(
                serde_json::to_value(options).unwrap(),
                serde_json::from_str::<serde_json::Value>(expected["json"].as_str().unwrap()).unwrap(),
                "{args:?}"
            ),
            Err(error) => assert_eq!(
                error.message.to_utf8().unwrap(),
                expected["error"]["message"].as_str().unwrap(),
                "{args:?}"
            ),
        }
    }
}
