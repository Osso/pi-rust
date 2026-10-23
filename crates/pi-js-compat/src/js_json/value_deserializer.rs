//! Serde visits the native authority directly; only actual JSON serialization projects numbers.
use super::{JsObject, JsString, JsValue, object_entries_lossless};
use serde::Deserializer as _;
use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess, Visitor,
};

type Error = serde_json::Error;

pub(super) fn from_value<T: DeserializeOwned>(value: &JsValue) -> Result<T, Error> {
    T::deserialize(ValueDeserializer(value))
}

struct ValueDeserializer<'a>(&'a JsValue);

impl ValueDeserializer<'_> {
    fn integer(&self, bits: u32, signed: bool) -> Result<f64, Error> {
        let Some(number) = self.0.as_f64() else {
            return Err(de::Error::custom("expected an integer number"));
        };
        let upper = 2_f64.powi(if signed { bits - 1 } else { bits } as i32);
        let lower = if signed { -upper } else { 0.0 };
        if !number.is_finite() || number.fract() != 0.0 || number < lower || number >= upper {
            return Err(de::Error::custom(format!(
                "number is not an exact {}{bits} integer",
                if signed { "i" } else { "u" }
            )));
        }
        Ok(number)
    }
}

macro_rules! deserialize_integer {
    ($method:ident, $visit:ident, $target:ty, $bits:expr, $signed:expr) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            visitor.$visit(self.integer($bits, $signed)? as $target)
        }
    };
}

impl<'de> de::Deserializer<'de> for ValueDeserializer<'_> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            JsValue::Null => visitor.visit_unit(),
            JsValue::Bool(value) => visitor.visit_bool(*value),
            JsValue::Number(value) => visit_number(*value, visitor),
            JsValue::String(value) => {
                let scalar = value
                    .to_utf8()
                    .map_err(|_| de::Error::custom("unpaired UTF-16 surrogate in scalar string"))?;
                visitor.visit_string(scalar)
            }
            JsValue::Array(values) => visit_array(values, visitor),
            JsValue::Object(values) => visit_object(values, visitor),
        }
    }

    deserialize_integer!(deserialize_i8, visit_i64, i64, 8, true);
    deserialize_integer!(deserialize_i16, visit_i64, i64, 16, true);
    deserialize_integer!(deserialize_i32, visit_i64, i64, 32, true);
    deserialize_integer!(deserialize_i64, visit_i64, i64, 64, true);
    deserialize_integer!(deserialize_i128, visit_i128, i128, 128, true);
    deserialize_integer!(deserialize_u8, visit_u64, u64, 8, false);
    deserialize_integer!(deserialize_u16, visit_u64, u64, 16, false);
    deserialize_integer!(deserialize_u32, visit_u64, u64, 32, false);
    deserialize_integer!(deserialize_u64, visit_u64, u64, 64, false);
    deserialize_integer!(deserialize_u128, visit_u128, u128, 128, false);

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            JsValue::Number(number) => visitor.visit_f32(*number as f32),
            _ => self.deserialize_any(visitor),
        }
    }
    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            JsValue::Number(number) => visitor.visit_f64(*number),
            _ => self.deserialize_any(visitor),
        }
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.0 == &JsValue::Null {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, Error> {
        if name == "$serde_json::private::RawValue" {
            // JsValue/JsString's raw-fragment visitor also accepts this native enum bridge.
            return visitor.visit_enum(NativeValueAccess(self.0));
        }
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.0 {
            JsValue::String(tag) => visitor.visit_enum(ExternalEnumAccess { tag, value: None }),
            JsValue::Object(object) if object.len() == 1 => {
                let (tag, value) = object.first().expect("one enum entry");
                visitor.visit_enum(ExternalEnumAccess {
                    tag,
                    value: Some(value),
                })
            }
            _ => Err(de::Error::custom("expected a string or single-key enum object")),
        }
    }
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
    serde::forward_to_deserialize_any! {
        bool char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

fn visit_number<'de, V: Visitor<'de>>(number: f64, visitor: V) -> Result<V::Value, Error> {
    // Preserve normal serde JSON integer dispatch for buffered/untagged types, but never erase -0.
    if number.is_finite() && number.fract() == 0.0 && !number.is_sign_negative() && number < 2_f64.powi(64) {
        return visitor.visit_u64(number as u64);
    }
    if number.is_finite() && number.fract() == 0.0 && number < 0.0 && number >= -2_f64.powi(63) {
        return visitor.visit_i64(number as i64);
    }
    visitor.visit_f64(number)
}

fn visit_array<'de, V: Visitor<'de>>(values: &[JsValue], visitor: V) -> Result<V::Value, Error> {
    let mut access = ArrayAccess(values.iter());
    let result = visitor.visit_seq(&mut access)?;
    if access.0.len() != 0 {
        return Err(de::Error::custom("unconsumed array elements"));
    }
    Ok(result)
}
fn visit_object<'de, V: Visitor<'de>>(values: &JsObject, visitor: V) -> Result<V::Value, Error> {
    let mut access = ObjectAccess {
        entries: object_entries_lossless(values).into_iter(),
        pending: None,
    };
    let result = visitor.visit_map(&mut access)?;
    if access.pending.is_some() || access.entries.len() != 0 {
        return Err(de::Error::custom("unconsumed object entries"));
    }
    Ok(result)
}

struct ArrayAccess<'a>(std::slice::Iter<'a, JsValue>);
impl<'de> SeqAccess<'de> for ArrayAccess<'_> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>, Error> {
        self.0
            .next()
            .map(|value| seed.deserialize(ValueDeserializer(value)))
            .transpose()
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

struct ObjectAccess<'a> {
    entries: std::vec::IntoIter<(&'a JsString, &'a JsValue)>,
    pending: Option<&'a JsValue>,
}
impl<'de> MapAccess<'de> for ObjectAccess<'_> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Error> {
        let Some((key, value)) = self.entries.next() else {
            return Ok(None);
        };
        self.pending = Some(value);
        seed.deserialize(MapKeyDeserializer(key)).map(Some)
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let value = self
            .pending
            .take()
            .ok_or_else(|| de::Error::custom("map value requested without a key"))?;
        seed.deserialize(ValueDeserializer(value))
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

struct MapKeyDeserializer<'a>(&'a JsString);
impl MapKeyDeserializer<'_> {
    fn numeric_text(&self) -> Result<String, Error> {
        let text = self
            .0
            .to_utf8()
            .map_err(|_| de::Error::custom("unpaired UTF-16 surrogate in numeric map key"))?;
        let valid_start = text
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'-');
        if !valid_start || text.trim() != text {
            return Err(de::Error::custom("expected a numeric map key"));
        }
        Ok(text)
    }
}
macro_rules! deserialize_numeric_key {
    ($method:ident) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            // Keys are strings, so numeric key parsing retains their decimal integer precision.
            let text = self.numeric_text()?;
            let mut parser = serde_json::Deserializer::from_reader(std::io::Cursor::new(text.into_bytes()));
            let value = parser.$method(visitor)?;
            parser.end()?;
            Ok(value)
        }
    };
}
impl<'de> de::Deserializer<'de> for MapKeyDeserializer<'_> {
    type Error = Error;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        ValueDeserializer(&JsValue::String(self.0.clone())).deserialize_any(visitor)
    }
    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.0 == "true" {
            return visitor.visit_bool(true);
        }
        if self.0 == "false" {
            return visitor.visit_bool(false);
        }
        self.deserialize_any(visitor)
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_some(self)
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(self, name: &'static str, visitor: V) -> Result<V::Value, Error> {
        if name == "$serde_json::private::RawValue" {
            return ValueDeserializer(&JsValue::String(self.0.clone())).deserialize_newtype_struct(name, visitor);
        }
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        ValueDeserializer(&JsValue::String(self.0.clone())).deserialize_enum(name, variants, visitor)
    }
    deserialize_numeric_key!(deserialize_i8);
    deserialize_numeric_key!(deserialize_i16);
    deserialize_numeric_key!(deserialize_i32);
    deserialize_numeric_key!(deserialize_i64);
    deserialize_numeric_key!(deserialize_i128);
    deserialize_numeric_key!(deserialize_u8);
    deserialize_numeric_key!(deserialize_u16);
    deserialize_numeric_key!(deserialize_u32);
    deserialize_numeric_key!(deserialize_u64);
    deserialize_numeric_key!(deserialize_u128);
    deserialize_numeric_key!(deserialize_f32);
    deserialize_numeric_key!(deserialize_f64);
    serde::forward_to_deserialize_any! {
        char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}

struct ExternalEnumAccess<'a> {
    tag: &'a JsString,
    value: Option<&'a JsValue>,
}
impl<'de, 'a> EnumAccess<'de> for ExternalEnumAccess<'a> {
    type Error = Error;
    type Variant = ExternalVariantAccess<'a>;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant), Error> {
        let tag = seed.deserialize(ValueDeserializer(&JsValue::String(self.tag.clone())))?;
        Ok((tag, ExternalVariantAccess(self.value)))
    }
}
struct ExternalVariantAccess<'a>(Option<&'a JsValue>);
impl<'de> VariantAccess<'de> for ExternalVariantAccess<'_> {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        match self.0 {
            None | Some(JsValue::Null) => Ok(()),
            Some(value) => serde::Deserialize::deserialize(ValueDeserializer(value)),
        }
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        seed.deserialize(ValueDeserializer(
            self.0.ok_or_else(|| de::Error::custom("enum payload is missing"))?,
        ))
    }
    fn tuple_variant<V: Visitor<'de>>(self, length: usize, visitor: V) -> Result<V::Value, Error> {
        ValueDeserializer(
            self.0
                .ok_or_else(|| de::Error::custom("enum tuple payload is missing"))?,
        )
        .deserialize_tuple(length, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
        ValueDeserializer(
            self.0
                .ok_or_else(|| de::Error::custom("enum struct payload is missing"))?,
        )
        .deserialize_struct("", fields, visitor)
    }
}

struct NativeValueAccess<'a>(&'a JsValue);
impl<'de, 'a> EnumAccess<'de> for NativeValueAccess<'a> {
    type Error = Error;
    type Variant = NativeValueVariant<'a>;
    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant), Error> {
        let tag = match self.0 {
            JsValue::Null => "null",
            JsValue::Bool(_) => "bool",
            JsValue::Number(_) => "number",
            JsValue::String(_) => "string",
            JsValue::Array(_) => "array",
            JsValue::Object(_) => "object",
        };
        let tag = seed.deserialize(tag.into_deserializer())?;
        Ok((tag, NativeValueVariant(self.0)))
    }
}
struct NativeValueVariant<'a>(&'a JsValue);
impl<'de> VariantAccess<'de> for NativeValueVariant<'_> {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        if self.0 == &JsValue::Null {
            Ok(())
        } else {
            Err(de::Error::custom("expected null"))
        }
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        match self.0 {
            JsValue::String(text) => seed.deserialize(de::value::SeqDeserializer::<_, Error>::new(
                text.units().iter().copied(),
            )),
            value => seed.deserialize(ValueDeserializer(value)),
        }
    }
    fn tuple_variant<V: Visitor<'de>>(self, _length: usize, _visitor: V) -> Result<V::Value, Error> {
        Err(de::Error::custom("native value is not a tuple variant"))
    }
    fn struct_variant<V: Visitor<'de>>(self, _fields: &'static [&'static str], _visitor: V) -> Result<V::Value, Error> {
        Err(de::Error::custom("native value is not a struct variant"))
    }
}

/// Called only by JsValue's raw-fragment visitor, not by arbitrary JSON consumers.
pub(super) fn decode_native_value<'de, A: EnumAccess<'de>>(access: A) -> Result<JsValue, A::Error> {
    let (tag, variant) = access.variant::<String>()?;
    match tag.as_str() {
        "null" => {
            variant.unit_variant()?;
            Ok(JsValue::Null)
        }
        "bool" => variant.newtype_variant().map(JsValue::Bool),
        "number" => variant.newtype_variant().map(JsValue::Number),
        "string" => variant
            .newtype_variant()
            .map(|units| JsValue::String(JsString::from_utf16(units))),
        "array" => variant.newtype_variant().map(JsValue::Array),
        "object" => variant.newtype_variant_seed(ObjectSeed).map(JsValue::Object),
        _ => Err(de::Error::custom("unknown native JS value kind")),
    }
}
struct ObjectSeed;
impl<'de> DeserializeSeed<'de> for ObjectSeed {
    type Value = JsObject;
    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<JsObject, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = JsObject;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a native JS object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<JsObject, A::Error> {
                let mut object = JsObject::new();
                while let Some((key, value)) = access.next_entry()? {
                    object.insert(key, value);
                }
                Ok(object)
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}
