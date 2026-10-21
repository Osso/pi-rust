//! JSON persistence; unknown record fields survive read/modify/write, as in TS.
use pi_js_compat::js_json;
use serde_json::Value;
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Storage(pub PathBuf);

impl Storage {
    fn read_json(&self, name: &str) -> Result<Option<Value>, String> {
        let path = self.0.join(name);
        match fs::read_to_string(&path) {
            Ok(text) => js_json::parse(&text).map(Some).map_err(|e| e.to_string()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("read {}: {e}", path.display())),
        }
    }
    fn write_json(&self, name: &str, value: &Value) -> Result<(), String> {
        fs::create_dir_all(&self.0).map_err(|e| e.to_string())?;
        fs::write(self.0.join(name), js_json::stringify_pretty(value)).map_err(|e| e.to_string())
    }
    pub fn load_machine(&self) -> Result<Option<Value>, String> {
        self.read_json("machine.json")
    }
    pub fn save_machine(&self, machine: &Value) -> Result<(), String> {
        self.write_json("machine.json", machine)
    }
    pub fn delete_machine(&self) -> Result<(), String> {
        remove_if_exists(&self.0.join("machine.json"))
    }
    pub fn load_instances(&self) -> Result<Vec<Value>, String> {
        match self.read_json("instances.json")? {
            None => Ok(vec![]),
            Some(Value::Array(records)) => Ok(records),
            Some(_) => Err("instances.json is not an array".into()),
        }
    }
    pub fn save_instances(&self, instances: &[Value]) -> Result<(), String> {
        self.write_json("instances.json", &Value::Array(instances.to_vec()))
    }
    pub fn get_instance(&self, id: &str) -> Result<Option<Value>, String> {
        Ok(self.load_instances()?.into_iter().find(|v| v["id"] == id))
    }
    pub fn upsert_instance(&self, instance: Value) -> Result<(), String> {
        let mut instances = self.load_instances()?;
        if let Some(index) = instances.iter().position(|v| v["id"] == instance["id"]) {
            instances[index] = instance;
        } else {
            instances.push(instance);
        }
        self.save_instances(&instances)
    }
    pub fn remove_instance(&self, id: &str) -> Result<(), String> {
        let instances = self
            .load_instances()?
            .into_iter()
            .filter(|v| v["id"] != id)
            .collect::<Vec<_>>();
        self.save_instances(&instances)
    }
}

pub fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("remove {}: {e}", path.display())),
    }
}
