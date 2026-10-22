//! JSON persistence; unknown record fields survive read/modify/write, as in TS.
use pi_coding_agent::utils::paths::node_join;
use pi_js_compat::{
    js_json::{self, JsString, JsValue, encode_node_utf8},
    node_fs::{node_file_write_error_record, node_fs_error, node_read_error, node_rm_error},
};
use std::{fs, io::Write, path::Path};

#[derive(Debug, Clone)]
pub struct Storage(pub JsString);
impl Storage {
    fn read_json(&self, name: &str) -> Result<Option<JsValue>, JsString> {
        let path = node_join(&self.0, name);
        let encoded = encode_node_utf8(&path);
        if !Path::new(&encoded).exists() {
            return Ok(None);
        }
        let bytes = fs::read(&encoded).map_err(|error| node_read_error(&error, &path))?;
        let text = String::from_utf8(bytes).map_err(|error| JsString::from(error.to_string()))?;
        js_json::parse_lossless(text)
            .map(Some)
            .map_err(js_json::JsonParseError::into_message)
    }
    fn write_json(&self, name: &str, value: &JsValue) -> Result<(), JsString> {
        let directory = encode_node_utf8(&self.0);
        if !Path::new(&directory).exists() {
            fs::create_dir_all(&directory).map_err(|error| node_fs_error(&error, "mkdir", &self.0))?;
        }
        let path = node_join(&self.0, name);
        let mut file =
            fs::File::create(encode_node_utf8(&path)).map_err(|error| node_fs_error(&error, "open", &path))?;
        file.write_all(js_json::stringify_lossless_pretty(value).as_bytes())
            .map_err(|error| node_file_write_error_record(&error).message)
    }
    pub fn load_machine(&self) -> Result<Option<JsValue>, JsString> {
        self.read_json("machine.json")
    }
    pub fn save_machine(&self, machine: &JsValue) -> Result<(), JsString> {
        self.write_json("machine.json", machine)
    }
    pub fn delete_machine(&self) -> Result<(), JsString> {
        let path = node_join(&self.0, "machine.json");
        let encoded = encode_node_utf8(&path);
        if !Path::new(&encoded).exists() {
            return Ok(());
        }
        fs::remove_file(encoded).map_err(|error| node_rm_error(&error, &path))
    }
    pub fn load_instances(&self) -> Result<Vec<JsValue>, JsString> {
        match self.read_json("instances.json")? {
            None => Ok(vec![]),
            Some(JsValue::Array(records)) => Ok(records),
            Some(_) => Err("instances.json is not an array".into()),
        }
    }
    pub fn save_instances(&self, instances: &[JsValue]) -> Result<(), JsString> {
        self.write_json("instances.json", &JsValue::Array(instances.to_vec()))
    }
    pub fn get_instance(&self, id: &JsString) -> Result<Option<JsValue>, JsString> {
        Ok(self.load_instances()?.into_iter().find(|v| v["id"] == *id))
    }
    pub fn upsert_instance(&self, instance: JsValue) -> Result<(), JsString> {
        let mut instances = self.load_instances()?;
        if let Some(index) = instances.iter().position(|v| v["id"] == instance["id"]) {
            instances[index] = instance;
        } else {
            instances.push(instance);
        }
        self.save_instances(&instances)
    }
    pub fn remove_instance(&self, id: &JsString) -> Result<(), JsString> {
        let instances = self
            .load_instances()?
            .into_iter()
            .filter(|v| v["id"] != *id)
            .collect::<Vec<_>>();
        self.save_instances(&instances)
    }
}
pub fn remove_if_exists(path: &JsString) -> Result<(), JsString> {
    let encoded = encode_node_utf8(path);
    if !Path::new(&encoded).exists() {
        return Ok(());
    }
    fs::remove_file(encoded).map_err(|error| node_fs_error(&error, "unlink", path))
}
