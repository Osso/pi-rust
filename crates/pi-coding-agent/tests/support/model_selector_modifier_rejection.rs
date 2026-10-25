use super::oracle;
use pi_ai::request::CallbackError;
use pi_coding_agent::{
    core::{
        auth_storage::AuthStorage,
        model_registry::{ExtensionOAuthConfig, ModelRegistry, ProviderConfigInput, ProviderModelInput},
    },
    modes::interactive::{
        components::model_selector::{ModelSelectorComponent, ModelSelectorUi},
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
};
use pi_js_compat::{
    abort::AbortReason,
    js_json,
    js_json::{JsString, JsValue},
    node_error::{NativeErrorClass, NativeNodeError},
};
use std::{
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

fn output_value(bytes: &[u8]) -> JsValue {
    let output = std::str::from_utf8(bytes).unwrap();
    js_json::parse_lossless(
        output
            .lines()
            .find_map(|line| line.strip_prefix("PROBE:"))
            .expect("process probe output"),
    )
    .unwrap()
}
#[test]
fn refresh_rejection_is_uncaught_after_selector_construction_like_frozen() {
    let source_lock = oracle::read_source_lock();
    oracle::verify_source_head(&source_lock);
    for kind in ["error", "value"] {
        let mut source = oracle::spawn_oracle_process(
            &source_lock,
            "model-selector-rejection-oracle.mts",
            "packages/coding-agent/src",
            &[],
            Stdio::piped(),
        );
        let request = js_json!({"kind":kind,"colors":REQUIRED_COLORS});
        source
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{}\n", js_json::stringify_lossless(&request)).as_bytes())
            .unwrap();
        let expected = source.wait_with_output().unwrap();
        assert_eq!(
            expected.status.code(),
            Some(23),
            "source {kind}: {}",
            String::from_utf8_lossy(&expected.stderr)
        );
        let actual = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "modifier_rejection::isolated_selector_rejection_probe",
                "--nocapture",
            ])
            .env("PI_SELECTOR_REJECTION_KIND", kind)
            .output()
            .unwrap();
        assert_eq!(
            actual.status.code(),
            Some(23),
            "native {kind}: stdout={} stderr={}",
            String::from_utf8_lossy(&actual.stdout),
            String::from_utf8_lossy(&actual.stderr)
        );
        let expected = output_value(&expected.stdout);
        assert_eq!(expected["constructed"], JsValue::Bool(true));
        assert_eq!(expected["renders"], JsValue::Number(0.0));
        assert_eq!(output_value(&actual.stdout), expected, "{kind}");
    }
}
struct Ui(Arc<AtomicUsize>);
impl ModelSelectorUi for Ui {
    fn request_render(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn snapshot_rejection(error: &CallbackError) -> JsValue {
    match error.thrown_value() {
        AbortReason::Error(error) | AbortReason::DomException(error) => {
            js_json!({"kind":"error","value":error.value()})
        }
        value => js_json!({"kind":"value","value":value.value()}),
    }
}
#[tokio::test(flavor = "current_thread")]
#[ignore = "private selector process rejection probe"]
async fn isolated_selector_rejection_probe() {
    let kind = std::env::var("PI_SELECTOR_REJECTION_KIND").unwrap();
    let raw = JsString::from("selector-") + JsString::from_utf16(vec![0xd800]);
    let rejection = if kind == "error" {
        CallbackError::from_rejection(
            NativeNodeError::from_constructor(NativeErrorClass::TypeError, raw.clone())
                .with_property("code", JsValue::String("E_SELECTOR".into()))
                .with_property("detail", js_json!({"marker":"original"}))
                .into(),
        )
    } else {
        CallbackError::from_rejection(AbortReason::Value(JsValue::String(raw)))
    };
    let constructed = Arc::new(AtomicBool::new(false));
    let renders = Arc::new(AtomicUsize::new(0));
    let completion = constructed.clone();
    let render_count = renders.clone();
    pi_coding_agent::utils::uncaught::set_process_crash_handler(Some(Arc::new(move |payload| {
        let Some(error) = payload.downcast_ref::<CallbackError>() else {
            std::process::exit(24)
        };
        let output = js_json!({"unhandled":true,"constructed":completion.load(Ordering::SeqCst),"renders":render_count.load(Ordering::SeqCst),"rejection":snapshot_rejection(error)});
        println!("\nPROBE:{}", js_json::stringify_lossless(&output));
        std::process::exit(23);
    })));
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    tokio::task::LocalSet::new().run_until(async move{
        let colors=REQUIRED_COLORS.iter().map(|name|((*name).into(),ColorValue::Index(0.0))).collect::<Vec<_>>();
        set_theme_instance(Arc::new(Theme::new(&colors,&[],ColorMode::Color256,None,None).unwrap()));
        let storage=AuthStorage::in_memory(serde_json::Map::new());let registry=ModelRegistry::in_memory(storage.clone()).unwrap();
        let definition=js_json!({"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32});
        registry.register_provider("selector-rejection",ProviderConfigInput {
            api:Some("openai-completions".into()),base_url:Some("https://fixture.invalid".into()),models:Some(vec![js_json::from_value::<ProviderModelInput>(&definition).unwrap()]),
            oauth:Some(ExtensionOAuthConfig{name:"Selector Fixture".into(),uses_callback_server:None,login:Arc::new(|_|Box::pin(async{unreachable!("unused login")})),refresh_token:Arc::new(|credential,_|Box::pin(async move{Ok(credential)})),get_api_key:Arc::new(|credential|Ok(credential.access.clone())),modify_models:Some(Arc::new(move|_,_|Err(rejection.clone())))}),..Default::default()
        }).unwrap();
        storage.set("selector-rejection",js_json!({"type":"oauth","access":"access","refresh":"refresh","expires":9999999999999_f64})).unwrap();
        let _selector=ModelSelectorComponent::new(Rc::new(Ui(renders.clone())),None,registry,vec![],Rc::new(|_|{}),Rc::new(||{}),None);
        constructed.store(true,Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        println!("\nPROBE:{}",js_json::stringify_lossless(&js_json!({"unhandled":false,"constructed":constructed.load(Ordering::SeqCst),"renders":renders.load(Ordering::SeqCst)})));
    }).await;
}
