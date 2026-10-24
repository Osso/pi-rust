//! Native Node Error data shared by IO, abort signals and provider boundaries.

use crate::abort::AbortReason;
use crate::js_json::{JsObject, JsString, JsValue, encode_node_utf8};
use crate::number::js_number_to_string;
use indexmap::IndexMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Built-in constructor identity; mutable Error.name never changes instanceof results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeErrorClass {
    Error,
    EvalError,
    RangeError,
    ReferenceError,
    SyntaxError,
    TypeError,
    URIError,
    AggregateError,
    SystemError,
}
impl NativeErrorClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::EvalError => "EvalError",
            Self::RangeError => "RangeError",
            Self::ReferenceError => "ReferenceError",
            Self::SyntaxError => "SyntaxError",
            Self::TypeError => "TypeError",
            Self::URIError => "URIError",
            Self::AggregateError => "AggregateError",
            Self::SystemError => "SystemError",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeNodeError {
    pub name: JsString,
    pub message: JsString,
    details: Box<NativeErrorDetails>,
}

/// Boxed so `Result<_, NativeNodeError>` stays small.
#[derive(Debug, Clone, PartialEq)]
struct NativeErrorDetails {
    constructor_class: NativeErrorClass,
    node_error_code: Option<JsString>,
    properties: IndexMap<JsString, AbortReason>,
}
impl NativeNodeError {
    pub fn new(name: impl Into<JsString>, message: impl Into<JsString>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
            details: Box::new(NativeErrorDetails {
                constructor_class: NativeErrorClass::Error,
                node_error_code: None,
                properties: IndexMap::new(),
            }),
        }
    }
    pub fn from_constructor(class: NativeErrorClass, message: impl Into<JsString>) -> Self {
        let mut error = Self::new(class.name(), message);
        error.details.constructor_class = class;
        error
    }
    /// Node's internal error constructors capture their code in the toString override.
    /// Assigning a code property to an ordinary Error does not install that override.
    pub fn from_node_constructor(
        class: NativeErrorClass,
        code: impl Into<JsString>,
        message: impl Into<JsString>,
    ) -> Self {
        let code = code.into();
        let mut error = Self::from_constructor(class, message);
        error.details.node_error_code = Some(code.clone());
        error.with_property("code", JsValue::String(code))
    }
    /// Node IO invalid-value rejection, preserving inspection and coded toString.
    pub fn invalid_argument_value(name: impl Into<JsString>, reason: &str, value: &JsString) -> Self {
        Self::from_node_constructor(
            NativeErrorClass::TypeError,
            "ERR_INVALID_ARG_VALUE",
            crate::node_fs::invalid_argument_value_message(name, reason, value),
        )
    }

    pub fn constructor_class(&self) -> NativeErrorClass {
        self.details.constructor_class
    }
    pub fn is_instance_of(&self, class: NativeErrorClass) -> bool {
        class == NativeErrorClass::Error || self.details.constructor_class == class
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
            self.details.properties.insert(name, AbortReason::Value(value));
        }
        self
    }
    /// Preserve cause branding without a second serialized property store.
    pub fn with_cause(mut self, cause: AbortReason) -> Self {
        self.details.properties.insert("cause".into(), cause);
        self
    }
    pub fn cause(&self) -> Option<&AbortReason> {
        self.details.properties.get(&JsString::from("cause"))
    }
    /// Error.prototype.toString, captured-code NodeError, or dynamic-code SystemError override.
    pub fn string_value(&self) -> JsString {
        if self.details.constructor_class == NativeErrorClass::SystemError {
            let code = self
                .details
                .properties
                .get(&JsString::from("code"))
                .map(|value| {
                    crate::js_json::to_js_string(&value.value()).expect("native error code is string-coercible")
                })
                .unwrap_or_else(|| "undefined".into());
            return crate::js_format!("{} [{}]: {}", self.name, code, self.message);
        }
        if let Some(code) = &self.details.node_error_code {
            return crate::js_format!("{} [{}]: {}", self.name, code, self.message);
        }
        if self.name.is_empty() {
            return self.message.clone();
        }
        if self.message.is_empty() {
            return self.name.clone();
        }
        crate::js_format!("{}: {}", self.name, self.message)
    }
    pub fn value(&self) -> JsValue {
        let mut value = JsObject::from([
            ("name".into(), JsValue::String(self.name.clone())),
            ("message".into(), JsValue::String(self.message.clone())),
        ]);
        value.extend(
            self.details
                .properties
                .iter()
                .map(|(key, value)| (key.clone(), value.value())),
        );
        JsValue::Object(value)
    }
}
/// Node ERR_OUT_OF_RANGE number inspection, including exponent separators.
pub fn format_range_number(value: f64) -> String {
    let text = js_number_to_string(value);
    if !value.is_finite() || value.fract() != 0.0 || value.abs() <= 4_294_967_296.0 {
        return text;
    }
    let (sign, digits) = text
        .strip_prefix('-')
        .map_or(("", text.as_str()), |digits| ("-", digits));
    // Node's ERR_OUT_OF_RANGE groups the rendered text, even exponent characters.
    let chunks = digits.as_bytes().rchunks(3).rev();
    let groups: Vec<_> = chunks
        .map(|chunk| std::str::from_utf8(chunk).expect("ASCII number rendering"))
        .collect();
    format!("{sign}{}", groups.join("_"))
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
        Self::from_node_constructor(NativeErrorClass::TypeError, error.code, error.message)
    }
}
impl Display for NativeNodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.message))
    }
}
impl Error for NativeNodeError {}
