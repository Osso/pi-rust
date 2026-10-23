//! Frozen ordinary-object inherited lookups, not own JSON properties.
//!
//! Derived from the pinned oracle's Node26.11.1/V8 14.6.202.34-node.37.
//! Every entry is truthy, non-string, and Number(value) is NaN. Text is only its
//! String(value) projection: never use it as a substitute for strict equality
//! against the original function/object. `__proto__` is read on an ordinary
//! object, yielding Object.prototype, not Object.prototype's own null prototype.

pub const INHERITED_OBJECT_PROPERTIES: &[(&str, &str)] = &[
    ("constructor", "function Object() { [native code] }"),
    ("__defineGetter__", "function __defineGetter__() { [native code] }"),
    ("__defineSetter__", "function __defineSetter__() { [native code] }"),
    ("hasOwnProperty", "function hasOwnProperty() { [native code] }"),
    ("__lookupGetter__", "function __lookupGetter__() { [native code] }"),
    ("__lookupSetter__", "function __lookupSetter__() { [native code] }"),
    ("isPrototypeOf", "function isPrototypeOf() { [native code] }"),
    (
        "propertyIsEnumerable",
        "function propertyIsEnumerable() { [native code] }",
    ),
    ("toString", "function toString() { [native code] }"),
    ("valueOf", "function valueOf() { [native code] }"),
    ("__proto__", "[object Object]"),
    ("toLocaleString", "function toLocaleString() { [native code] }"),
];

pub fn inherited_property_string(name: &str) -> Option<&'static str> {
    INHERITED_OBJECT_PROPERTIES
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, text)| *text)
}
