//! Native Node Error data shared by IO, abort signals and provider boundaries.

use crate::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq)]
pub struct NativeNodeError {
    pub name: JsString,
    pub message: JsString,
    properties: JsObject,
}
impl NativeNodeError {
    pub fn new(name: impl Into<JsString>, message: impl Into<JsString>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
            properties: JsObject::default(),
        }
    }
    /// Attach native error data; human fields remain string authorities and JS stacks are omitted.
    pub fn with_property(mut self, name: impl Into<JsString>, value: JsValue) -> Self {
        let name = name.into();
        if name == "stack" {
            return self;
        }
        if name == "name" || name == "message" {
            let JsValue::String(value) = value else {
                panic!("native Error name/message must be JS strings")
            };
            if name == "name" {
                self.name = value;
            } else {
                self.message = value;
            }
        } else {
            self.properties.insert(name, value);
        }
        self
    }
    pub fn value(&self) -> JsValue {
        let mut value = JsObject::from([
            ("name".into(), JsValue::String(self.name.clone())),
            ("message".into(), JsValue::String(self.message.clone())),
        ]);
        value.extend(self.properties.clone());
        JsValue::Object(value)
    }
}
impl From<crate::node_fs::NodeFsError> for NativeNodeError {
    fn from(error: crate::node_fs::NodeFsError) -> Self {
        let mut value = Self::new(error.name, error.message)
            .with_property("code", JsValue::String(error.code.into()))
            .with_property("syscall", JsValue::String(error.syscall));
        if let Some(errno) = error.errno {
            value = value.with_property("errno", JsValue::Number(f64::from(errno)));
        }
        if let Some(path) = error.path {
            value = value.with_property("path", JsValue::String(path));
        }
        value
    }
}
impl From<crate::node_fs::NodePathArgumentError> for NativeNodeError {
    fn from(error: crate::node_fs::NodePathArgumentError) -> Self {
        Self::new(error.name, error.message).with_property("code", JsValue::String(error.code.into()))
    }
}
impl Display for NativeNodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}
impl Error for NativeNodeError {}
