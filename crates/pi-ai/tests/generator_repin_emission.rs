use crate::shared_oracle as oracle;

use pi_ai::model_catalog_generator::{Generator, HttpResponse};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue, parse_lossless, stringify_lossless},
};
use std::path::{Path, PathBuf};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "pi-emission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in [
            "scripts",
            "src/providers",
            "crates/pi-ai/data/providers",
            "crates/pi-ai/src/providers",
        ] {
            std::fs::create_dir_all(path.join(directory)).unwrap();
        }
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn compile_and_read_generated_include(root: &Path) -> String {
    let source = root.join("consumer.rs");
    let binary = root.join("consumer");
    std::fs::write(
        &source,
        r#"include!("crates/pi-ai/src/providers/model_catalogs_generated.rs");
fn main() {
    let (_, json) = PROVIDER_CATALOGS.iter().find(|(id, _)| *id == "anthropic").unwrap();
    print!("{json}");
}"#,
    )
    .unwrap();
    let compilation = std::process::Command::new("rustc")
        .current_dir(root)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        compilation.status.success(),
        "{}",
        String::from_utf8(compilation.stderr).unwrap()
    );
    let output = std::process::Command::new(binary).current_dir(root).output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout).unwrap()
}

fn responses(model: JsValue) -> JsValue {
    js_json!({"https://models.dev/api.json":{"body":{"anthropic":{"models":{"fixture":model}}}},"https://openrouter.ai/api/v1/models":{"body":{"data":[]}},"https://ai-gateway.vercel.sh/v1/models":{"body":{"data":[]}}})
}

// Frozen emitModel:2146–2180 interpolates strings and literal expressions;
// writeFileSync:2200 converts lone UTF-16 units at the file boundary only.
#[test]
fn artifact_values_match_real_written_and_evaluated_frozen_provider_module() {
    let source_root = Sandbox::new();
    let native_root = Sandbox::new();
    let name = JsString::from_utf16(vec![b'h' as u16, 0xd800, b'i' as u16]);
    let responses = responses(
        js_json!({"name":name,"tool_call":true,"cost":{"input":"0x10","output":"2e1","cache_read":"0o10","cache_write":"0b11"},"limit":{"context":"0b1000000000000","output":"1e2"}}),
    );
    let mut oracle = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    let expected = oracle.request(
        &js_json!({"operation":"emission","responses":responses,"filesystemRoot":source_root.path().to_str().unwrap()}),
    );
    assert_eq!(expected["ok"], js_json!(true), "{expected:?}");
    let path = source_root.path().join("src/providers/anthropic.models.ts");
    let expected_model = &expected["loaded"][path.to_str().unwrap()]["fixture"];
    assert_eq!(expected_model["name"], js_json!("h\u{fffd}i"));
    assert_eq!(expected_model["cost"]["input"], js_json!(16));
    let mut fetch = |url: &str| {
        Ok(HttpResponse {
            status: 200,
            body: stringify_lossless(&responses[url]["body"]),
        })
    };
    let catalog = Generator::new(&mut fetch).generate(native_root.path()).unwrap();
    assert_eq!(
        catalog["anthropic"]["fixture"]["name"],
        JsValue::String(name),
        "intermediate stays lossless"
    );
    let actual = parse_lossless(
        std::fs::read_to_string(native_root.path().join("crates/pi-ai/data/providers/anthropic.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        actual["fixture"], *expected_model,
        "native artifact must represent the evaluated flat provider export"
    );
    let included = compile_and_read_generated_include(native_root.path());
    assert_eq!(
        parse_lossless(&included).unwrap(),
        actual,
        "compiled include must embed the emitted artifact"
    );
    let models = pi_ai::providers::models_data::read_provider_catalog(&included).unwrap();
    let model = &models["fixture"];
    assert_eq!(pi_js_compat::js_json::to_value(model).unwrap(), *expected_model);
    assert!(pi_ai::models::has_api(model, "anthropic-messages"));
    let usage = pi_ai::types::Usage {
        input: 3.0,
        output: 2.0,
        ..Default::default()
    };
    let cost = pi_ai::models::calculate_cost(model, &usage);
    assert_eq!(cost.input, 48.0 / 1_000_000.0);
    assert_eq!(cost.output, 40.0 / 1_000_000.0);
}

#[test]
fn numeric_literal_separators_materialize_the_evaluated_source_value() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    for input in [
        "1_024",
        "0x1_f",
        "0o1_0",
        "0b1_0",
        "1.2_5e1_0",
        "+0x10",
        "-0b10",
        ".5",
        "5.",
    ] {
        let source_root = Sandbox::new();
        let native_root = Sandbox::new();
        let responses = responses(js_json!({"tool_call":true,"cost":{"input":input}}));
        let expected=source.request(&js_json!({"operation":"emission","responses":responses,"filesystemRoot":source_root.path().to_str().unwrap()}));
        assert_eq!(expected["ok"], js_json!(true), "{input}: {expected:?}");
        let source_path = source_root.path().join("src/providers/anthropic.models.ts");
        let expected_model = &expected["loaded"][source_path.to_str().unwrap()]["fixture"];
        let mut fetch = |url: &str| {
            Ok(HttpResponse {
                status: 200,
                body: stringify_lossless(&responses[url]["body"]),
            })
        };
        Generator::new(&mut fetch).generate(native_root.path()).unwrap();
        let actual = parse_lossless(
            std::fs::read_to_string(native_root.path().join("crates/pi-ai/data/providers/anthropic.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(actual["fixture"], *expected_model, "input={input}");
    }
}

#[test]
fn emission_rejects_injection_nonfinite_wrong_typed_literals_and_opaque_surrogates() {
    for (field, value, expected_field) in [
        ("name", js_json!("quote\"break"), "name"),
        ("name", js_json!("raw\nline"), "name"),
        ("cost", js_json!({"input":"1+2"}), "cost.input"),
        ("cost", js_json!({"input":"++1"}), "cost.input"),
        ("cost", js_json!({"input":"--1"}), "cost.input"),
        ("cost", js_json!({"input":"-+1"}), "cost.input"),
        ("cost", js_json!({"input":"NaN"}), "cost.input"),
        ("cost", js_json!({"input":"Infinity"}), "cost.input"),
        ("cost", js_json!({"input":"true"}), "cost.input"),
        ("limit", js_json!({"context":"'text'"}), "contextWindow"),
        ("cost", js_json!({"input":{"toString":0}}), "cost.input"),
    ] {
        let root = Sandbox::new();
        let mut model = js_json!({"tool_call":true,"name":"fixture"});
        model[field] = value;
        let responses = responses(model);
        let mut fetch = |url: &str| {
            Ok(HttpResponse {
                status: 200,
                body: stringify_lossless(&responses[url]["body"]),
            })
        };
        let error = Generator::new(&mut fetch)
            .generate(root.path())
            .expect_err("invalid emission must never poison a catalog");
        let rendered = error.to_string();
        assert!(
            rendered.contains("anthropic") && rendered.contains("fixture") && rendered.contains(expected_field),
            "missing provider/model/field in {rendered}"
        );
        assert!(!root.path().join("crates/pi-ai/data/providers/anthropic.json").exists());
    }
    let root = Sandbox::new();
    let id = JsString::from_utf16(vec![0xd800]);
    let mut responses = responses(js_json!({"tool_call":true}));
    responses["https://models.dev/api.json"]["body"]["anthropic"]["models"] =
        JsValue::Object([(id, js_json!({"tool_call":true}))].into_iter().collect());
    let mut fetch = |url: &str| {
        Ok(HttpResponse {
            status: 200,
            body: stringify_lossless(&responses[url]["body"]),
        })
    };
    let error = Generator::new(&mut fetch)
        .generate(root.path())
        .expect_err("opaque ID must reject lone units");
    assert!(error.to_string().contains("id"), "{error}");
}

#[test]
fn string_interpolation_uses_tostring_then_javascript_escape_grammar() {
    let mut source = oracle::TsOracle::spawn("generator-repin-oracle.mts", "packages/ai/scripts/generate-models.ts");
    for name in [
        js_json!(17),
        js_json!(true),
        js_json!(["a", "b"]),
        js_json!({"x":1}),
        js_json!(r#"line\nquote\""#),
        js_json!(r"\x41\u0042\u{1F600}\q"),
        js_json!(r"\uD800"),
    ] {
        let source_root = Sandbox::new();
        let native_root = Sandbox::new();
        let responses = responses(js_json!({"tool_call":true,"name":name}));
        let expected=source.request(&js_json!({"operation":"emission","responses":responses,"filesystemRoot":source_root.path().to_str().unwrap()}));
        assert_eq!(expected["ok"], js_json!(true), "{expected:?}");
        let source_path = source_root.path().join("src/providers/anthropic.models.ts");
        let expected_model = &expected["loaded"][source_path.to_str().unwrap()]["fixture"];
        assert!(expected_model.as_object().is_some(), "{expected:?}");
        let mut fetch = |url: &str| {
            Ok(HttpResponse {
                status: 200,
                body: stringify_lossless(&responses[url]["body"]),
            })
        };
        Generator::new(&mut fetch).generate(native_root.path()).unwrap();
        let actual = parse_lossless(
            std::fs::read_to_string(native_root.path().join("crates/pi-ai/data/providers/anthropic.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(actual["fixture"], *expected_model, "name={name:?}");
    }
}
