use crate::shared_oracle as oracle;
use pi_coding_agent::modes::interactive::theme::{self, REQUIRED_COLORS, ThemeColor};
use pi_js_compat::{js_json::JsString, process_env};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn palette(name: &str, index: u32) -> Value {
    let colors: serde_json::Map<_, _> = REQUIRED_COLORS
        .iter()
        .map(|name| ((*name).to_owned(), json!(index)))
        .collect();
    json!({"name":name,"colors":colors})
}
#[tokio::test(flavor = "current_thread")]
async fn real_theme_watcher_lifecycle_matches_frozen() {
    tokio::task::LocalSet::new().run_until(async {
  let directory=tempfile::tempdir().unwrap();
  let root=directory.path();
  std::fs::create_dir(root.join("theme")).unwrap();
  std::fs::create_dir_all(root.join("dist/modes/interactive/theme")).unwrap();
  std::fs::create_dir_all(root.join("agent/themes")).unwrap();
  std::fs::write(root.join("package.json"),r#"{"name":"fixture","version":"1","piConfig":{"appName":"pi"}}"#).unwrap();
  process_env::set("PI_PACKAGE_DIR",root.to_str().unwrap());
  process_env::set("PI_CODING_AGENT_DIR",root.join("agent").to_str().unwrap());
  let watched=root.join("agent/themes/watch.json");
  let sibling=root.join("agent/themes/other.json");
  let swap=root.join("agent/themes/swap.json");
  let mut setup=serde_json::Map::new();
  for (path,name,index) in [(root.join("theme/dark.json"),"dark",0),(root.join("theme/light.json"),"light",15),(root.join("dist/modes/interactive/theme/dark.json"),"dark",0),(root.join("dist/modes/interactive/theme/light.json"),"light",15),(watched.clone(),"watch",1),(sibling.clone(),"other",1)]{
   setup.insert(path.display().to_string(),palette(name,index));
  }
  let steps=json!([
   {"op":"snapshot"},
   {"op":"write","path":watched,"value":palette("renamed",2)},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"write","path":sibling,"value":palette("other",3)},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"write","path":watched,"value":palette("renamed",3)},{"op":"wait","ms":40},
   {"op":"write","path":watched,"value":palette("renamed",4)},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"write","path":watched,"raw":"{"},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"write","path":swap,"value":palette("renamed",5)},{"op":"rename","from":swap,"to":watched},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"remove","path":watched},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"write","path":watched,"value":palette("renamed",6)},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"write","path":watched,"value":palette("renamed",7)},{"op":"set","name":"dark"},{"op":"wait","ms":250},{"op":"snapshot"},
   {"op":"set","name":"watch"},{"op":"write","path":watched,"value":palette("renamed",8)},{"op":"stop"},{"op":"wait","ms":250},{"op":"snapshot"}
  ]);
  for (path,value) in &setup{std::fs::write(path,serde_json::to_vec(value).unwrap()).unwrap();}
  theme::stop_theme_watcher();theme::set_registered_themes(vec![]).unwrap();
  let callbacks=Rc::new(RefCell::new(Vec::<String>::new()));
  let record=callbacks.clone();
  theme::on_theme_change(Rc::new(move||record.borrow_mut().push(theme::current_theme().get_fg_ansi(ThemeColor::Accent).into())));
  theme::init_theme(Some(&"watch".into()),true).unwrap();
  let mut frames=vec![];
  for step in steps.as_array().unwrap(){
   match step["op"].as_str().unwrap(){
    "write"=>std::fs::write(step["path"].as_str().unwrap(),step["raw"].as_str().map(str::to_owned).unwrap_or_else(||serde_json::to_string(&step["value"]).unwrap())).unwrap(),
    "rename"=>std::fs::rename(step["from"].as_str().unwrap(),step["to"].as_str().unwrap()).unwrap(),
    "remove"=>std::fs::remove_file(step["path"].as_str().unwrap()).unwrap(),
    "wait"=>tokio::time::sleep(Duration::from_millis(step["ms"].as_u64().unwrap())).await,
    "set"=>{theme::set_theme(&JsString::from(step["name"].as_str().unwrap()),true).unwrap();},
    "stop"=>theme::stop_theme_watcher(),
    "snapshot"=>{
     let before_access=callbacks.borrow().clone();
     let current=theme::current_theme();
     frames.push(json!({"callbacks":before_access,"name":current.name,"accent":current.get_fg_ansi(ThemeColor::Accent),"registered":theme::get_theme_by_name(&"watch".into()).map(|t|t.get_fg_ansi(ThemeColor::Accent).to_owned())}));
    },
    value=>panic!("unknown operation {value}")
   }
  }
  theme::stop_theme_watcher();
  let mut source=oracle::TsOracle::spawn_with_environment("theme-lifecycle-oracle.mts","packages/coding-agent/src/modes/interactive/theme/theme.ts", &[("PI_PACKAGE_DIR",root.as_os_str().to_owned()),("PI_CODING_AGENT_DIR",root.join("agent").into_os_string())]);
  let expected:Value=source.request(&json!({"id":1,"setup":setup,"steps":steps}));
  assert_eq!(expected["ok"],true,"{expected}");
  assert_eq!(json!(frames),expected["output"]);
  process_env::clear();
 }).await;
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
