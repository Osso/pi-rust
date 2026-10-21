//! JSON persistence; unknown record fields survive read/modify/write, as in TS.
use pi_js_compat::{
    js_json,
    node_fs::{node_file_write_error_record, node_fs_error, node_read_error, node_rm_error},
};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Storage(pub PathBuf);

impl Storage {
    fn read_json(&self, name: &str) -> Result<Option<Value>, String> {
        let path = crate::config::join_path(&self.0, name);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|error| node_read_error(&error, &path.to_string_lossy()))?;
        let text = String::from_utf8_lossy(&bytes);
        js_json::parse(&text).map(Some).map_err(|error| error.to_string())
    }
    fn write_json(&self, name: &str, value: &Value) -> Result<(), String> {
        if !self.0.exists() {
            fs::create_dir_all(&self.0).map_err(|error| node_fs_error(&error, "mkdir", &self.0.to_string_lossy()))?;
        }
        let path = crate::config::join_path(&self.0, name);
        let mut file =
            fs::File::create(&path).map_err(|error| node_fs_error(&error, "open", &path.to_string_lossy()))?;
        file.write_all(js_json::stringify_pretty(value).as_bytes())
            .map_err(|error| node_file_write_error_record(&error).message)
    }
    pub fn load_machine(&self) -> Result<Option<Value>, String> {
        self.read_json("machine.json")
    }
    pub fn save_machine(&self, machine: &Value) -> Result<(), String> {
        self.write_json("machine.json", machine)
    }
    pub fn delete_machine(&self) -> Result<(), String> {
        let path = crate::config::join_path(&self.0, "machine.json");
        if !path.exists() {
            return Ok(());
        }
        fs::remove_file(&path).map_err(|error| node_rm_error(&error, &path.to_string_lossy()))
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
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|error| node_fs_error(&error, "unlink", &path.to_string_lossy()))
}
