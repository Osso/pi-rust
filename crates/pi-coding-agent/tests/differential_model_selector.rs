use crate::shared_oracle as oracle;
use pi_coding_agent::{
    core::{auth_storage::AuthStorage, model_registry::ModelRegistry, model_resolver::ScopedModel},
    modes::interactive::{
        components::model_selector::{ModelSelectorComponent, ModelSelectorUi},
        theme::{ColorMode, ColorValue, REQUIRED_COLORS, Theme, set_theme_instance},
    },
};
use pi_js_compat::js_json::JsString;
use pi_tui::{Component, Focusable};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
struct Ui(Cell<usize>);
impl ModelSelectorUi for Ui {
    fn request_render(&self) {
        self.0.set(self.0.get() + 1);
    }
}
fn install_theme() -> Arc<Theme> {
    let colors: Vec<_> = REQUIRED_COLORS
        .iter()
        .map(|key| ((*key).into(), ColorValue::Index(0.0)))
        .collect();
    let theme = Arc::new(Theme::new(&colors, &[], ColorMode::Color256, None, None).unwrap());
    set_theme_instance(theme.clone());
    theme
}
fn catalog(name: &str, count: usize) -> String {
    let models:Vec<_>=(0..count).map(|index|json!({"id":format!("model-{index:02}"),"name":format!("{name} {index}"),"reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":4096,"maxTokens":1024})).collect();
    serde_json::to_string(&json!({"providers":{"zeta":{"baseUrl":"https://example.test/v1","api":"openai-completions","apiKey":"fixture","models":models},"alpha":{"baseUrl":"https://example.test/v1","api":"openai-completions","apiKey":"fixture","models":[{"id":"first","name":"Alpha refreshed"}]}}})).unwrap()
}
fn text(step: &Value) -> JsString {
    if let Some(units) = step["units"].as_array() {
        JsString::from_utf16(units.iter().map(|n| n.as_u64().unwrap() as u16).collect())
    } else {
        step["value"].as_str().unwrap().into()
    }
}
async fn compare(source: &mut oracle::TsOracle, mut request: Value) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models.json");
    request["path"] = json!(path);
    std::fs::write(&path, request["initial"].as_str().unwrap()).unwrap();
    let registry = ModelRegistry::create(
        AuthStorage::in_memory([] as [(&str, Value); 0]),
        Some(path.to_str().unwrap().into()),
    )
    .unwrap();
    let find = |value: &Value| {
        registry
            .find(value["provider"].as_str().unwrap(), value["id"].as_str().unwrap())
            .unwrap()
    };
    let current = (!request["current"].is_null()).then(|| find(&request["current"]));
    let scoped = request["scoped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| ScopedModel {
            model: find(value),
            thinking_level: None,
        })
        .collect();
    std::fs::write(&path, request["refreshed"].as_str().unwrap()).unwrap();
    let ui = Rc::new(Ui(Cell::new(0)));
    let events = Rc::new(RefCell::new(Vec::<Value>::new()));
    let selected = events.clone();
    let canceled = events.clone();
    install_theme();
    let component = ModelSelectorComponent::new(
        ui.clone(),
        current,
        registry,
        scoped,
        Rc::new(move |model| {
            selected
                .borrow_mut()
                .push(json!({"provider":model.provider,"id":model.id,"name":model.name.units()}))
        }),
        Rc::new(move || canceled.borrow_mut().push(json!("cancel"))),
        request["query"].as_str().map(JsString::from).as_ref(),
    );
    let mut width = request["width"].as_u64().unwrap() as usize;
    let snapshot = |component: &Rc<RefCell<ModelSelectorComponent>>, width| {
        let mut component = component.borrow_mut();
        let input = component.get_search_input();
        json!({"lines":component.render(width).iter().map(|line|line.units()).collect::<Vec<_>>(),"events":events.borrow().clone(),"renders":ui.0.get(),"query":input.borrow().get_value().units(),"focused":input.borrow().focused(),"children":component.children().len()})
    };
    let mut frames = vec![snapshot(&component, width)];
    let mut added: Vec<pi_tui::tui::ComponentRef> = vec![];
    for step in request["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "settle" => tokio::time::sleep(std::time::Duration::from_millis(1)).await,
            "key" => component.borrow_mut().handle_input(&text(step)),
            "inputKey" => {
                let input = component.borrow().get_search_input();
                input.borrow_mut().handle_input(text(step));
            }
            "setInput" => {
                let input = component.borrow().get_search_input();
                input.borrow_mut().set_value(text(step));
            }
            "focus" => component.borrow_mut().set_focused(step["value"].as_bool().unwrap()),
            "width" => width = step["value"].as_u64().unwrap() as usize,
            "invalidate" => component.borrow_mut().invalidate(),
            "add" => {
                let child = pi_tui::tui::component(pi_tui::Text::new(text(step), 0, 0));
                component.borrow_mut().add_child(child.clone());
                added.push(child);
            }
            "remove" => component
                .borrow_mut()
                .remove_child(&added[step["index"].as_u64().unwrap() as usize]),
            "clear" => component.borrow_mut().clear(),
            value => panic!("unsupported operation {value}"),
        }
        frames.push(snapshot(&component, width));
    }
    let expected: Value = source.request(&request);
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(json!(frames), expected["output"], "case {}", request["name"]);
}
#[tokio::test(flavor = "current_thread")]
async fn model_selector_refresh_loading_input_and_render_match_frozen_registry() {
    tokio::task::LocalSet::new().run_until(async{
  let mut source=oracle::TsOracle::spawn("model-selector-oracle.mts","packages/coding-agent/src");
  let base=json!({"name":"refresh-and-scope","initial":catalog("Old",13),"refreshed":catalog("Updated",13),"colors":REQUIRED_COLORS,"current":{"provider":"zeta","id":"model-08"},"scoped":[{"provider":"zeta","id":"model-08"},{"provider":"zeta","id":"model-03"}],"width":80,"steps":[{"op":"settle"},{"op":"focus","value":true},{"op":"key","value":"\t"},{"op":"key","value":"\x1b[B"},{"op":"key","value":"\r"},{"op":"key","value":"\x1b"},{"op":"inputKey","value":"\r"},{"op":"width","value":12},{"op":"invalidate"}]});
  compare(&mut source,base.clone()).await;
  let mut query=base.clone();query["name"]=json!("selection-retained-under-query");query["scoped"]=json!([]);query["steps"]=json!([{"op":"settle"},{"op":"key","value":"\x1b[B"},{"op":"key","value":"\x1b[B"},{"op":"key","value":"model"},{"op":"key","units":[55296]},{"op":"key","value":"\r"}]);compare(&mut source,query).await;
  let mut early=base.clone();early["name"]=json!("input-before-load");early["scoped"]=json!([]);early["steps"]=json!([{"op":"key","value":"missing"},{"op":"settle"},{"op":"key","value":"\r"}]);compare(&mut source,early).await;
  let mut initial=base.clone();initial["name"]=json!("initial-query-live-child-and-wrap");initial["query"]=json!("model");initial["steps"]=json!([{"op":"key","value":"-extra"},{"op":"settle"},{"op":"focus","value":true},{"op":"key","value":"\x1b[A"},{"op":"key","value":"\x1b[B"},{"op":"setInput","value":"alpha"},{"op":"inputKey","value":"\r"},{"op":"key","value":"!"},{"op":"inputKey","value":"\r"}]);compare(&mut source,initial).await;
  let mut inherited=base.clone();inherited["name"]=json!("inherited-container-live-children");inherited["scoped"]=json!([]);inherited["steps"]=json!([{"op":"add","value":"extra"},{"op":"settle"},{"op":"remove","index":0},{"op":"clear"},{"op":"key","value":"alpha"},{"op":"invalidate"}]);compare(&mut source,inherited).await;
  let mut cleared=base.clone();cleared["name"]=json!("clear-before-constructor-continuation");cleared["steps"]=json!([{"op":"clear"},{"op":"settle"},{"op":"focus","value":true},{"op":"inputKey","value":"\r"}]);compare(&mut source,cleared).await;
  let mut empty=base.clone();empty["name"]=json!("empty-models-no-match");empty["scoped"]=json!([]);empty["refreshed"]=json!("{\"providers\":{}}");empty["steps"]=json!([{"op":"settle"},{"op":"key","value":"\x1b[A"},{"op":"key","value":"\x1b[B"},{"op":"key","value":"\r"},{"op":"key","value":"\x1b"}]);compare(&mut source,empty).await;
  let mut raw=base.clone();raw["name"]=json!("raw-refreshed-model-display-name");raw["refreshed"]=json!(catalog("Updated",13).replace("Updated 8","Updated \\ud800 8"));raw["steps"]=json!([{"op":"settle"},{"op":"key","value":"\r"},{"op":"width","value":0}]);compare(&mut source,raw).await;
  let mut error=base;error["name"]=json!("load-error-still-shows-scoped");error["refreshed"]=json!("{");error["steps"]=json!([{"op":"settle"},{"op":"key","value":"\t"},{"op":"key","value":"\r"}]);compare(&mut source,error).await;
 }).await;
}
