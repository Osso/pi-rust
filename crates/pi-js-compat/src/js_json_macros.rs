// Adapted from serde_json 1.0.151 src/macros.rs under its MIT license.
// Permission is hereby granted, free of charge, to any
// person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the
// Software without restriction, including without
// limitation the rights to use, copy, modify, merge,
// publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software
// is furnished to do so, subject to the following
// conditions:
//
// The above copyright notice and this permission notice
// shall be included in all copies or substantial portions
// of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
// ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
// TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
// PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
// SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
// OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
// IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

/// Construct lossless JSON without an intermediate serde_json::Value.
#[macro_export]
macro_rules! js_json { ($($json:tt)+) => { $crate::js_json_internal!($($json)+) }; }

#[macro_export]
#[doc(hidden)]
macro_rules! js_json_internal {
    (@array [$($elems:expr,)*]) => {
        ::std::vec![$($elems,)*]
    };
    (@array [$($elems:expr),*]) => {
        ::std::vec![$($elems),*]
    };
    (@array [$($elems:expr,)*] null $($rest:tt)*) => {
        $crate::js_json_internal!(@array [$($elems,)* $crate::js_json_internal!(null)] $($rest)*)
    };
    (@array [$($elems:expr,)*] true $($rest:tt)*) => {
        $crate::js_json_internal!(@array [$($elems,)* $crate::js_json_internal!(true)] $($rest)*)
    };
    (@array [$($elems:expr,)*] false $($rest:tt)*) => {
        $crate::js_json_internal!(@array [$($elems,)* $crate::js_json_internal!(false)] $($rest)*)
    };
    (@array [$($elems:expr,)*] [$($array:tt)*] $($rest:tt)*) => {
        $crate::js_json_internal!(@array [$($elems,)* $crate::js_json_internal!([$($array)*])] $($rest)*)
    };
    (@array [$($elems:expr,)*] {$($map:tt)*} $($rest:tt)*) => {
        $crate::js_json_internal!(@array [$($elems,)* $crate::js_json_internal!({$($map)*})] $($rest)*)
    };
    (@array [$($elems:expr,)*] $next:expr, $($rest:tt)*) => {
        $crate::js_json_internal!(@array [$($elems,)* $crate::js_json_internal!($next),] $($rest)*)
    };
    (@array [$($elems:expr,)*] $last:expr) => {
        $crate::js_json_internal!(@array [$($elems,)* $crate::js_json_internal!($last)])
    };
    (@array [$($elems:expr),*] , $($rest:tt)*) => {
        $crate::js_json_internal!(@array [$($elems,)*] $($rest)*)
    };
    (@array [$($elems:expr),*] $unexpected:tt $($rest:tt)*) => {
        $crate::js_json_unexpected!($unexpected)
    };
    (@object $object:ident () () ()) => {};
    (@object $object:ident [$($key:tt)+] ($value:expr) , $($rest:tt)*) => {
        let _ = $object.insert(($($key)+).into(), $value);
        $crate::js_json_internal!(@object $object () ($($rest)*) ($($rest)*));
    };
    (@object $object:ident [$($key:tt)+] ($value:expr) $unexpected:tt $($rest:tt)*) => {
        $crate::js_json_unexpected!($unexpected);
    };
    (@object $object:ident [$($key:tt)+] ($value:expr)) => {
        let _ = $object.insert(($($key)+).into(), $value);
    };
    (@object $object:ident ($($key:tt)+) (: null $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object [$($key)+] ($crate::js_json_internal!(null)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: true $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object [$($key)+] ($crate::js_json_internal!(true)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: false $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object [$($key)+] ($crate::js_json_internal!(false)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: [$($array:tt)*] $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object [$($key)+] ($crate::js_json_internal!([$($array)*])) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: {$($map:tt)*} $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object [$($key)+] ($crate::js_json_internal!({$($map)*})) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: $value:expr , $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object [$($key)+] ($crate::js_json_internal!($value)) , $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: $value:expr) $copy:tt) => {
        $crate::js_json_internal!(@object $object [$($key)+] ($crate::js_json_internal!($value)));
    };
    (@object $object:ident ($($key:tt)+) (:) $copy:tt) => {
        $crate::js_json_internal!();
    };
    (@object $object:ident ($($key:tt)+) () $copy:tt) => {
        $crate::js_json_internal!();
    };
    (@object $object:ident () (: $($rest:tt)*) ($colon:tt $($copy:tt)*)) => {
        $crate::js_json_unexpected!($colon);
    };
    (@object $object:ident ($($key:tt)*) (, $($rest:tt)*) ($comma:tt $($copy:tt)*)) => {
        $crate::js_json_unexpected!($comma);
    };
    (@object $object:ident () (($key:expr) : $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object ($key) (: $($rest)*) (: $($rest)*));
    };
    (@object $object:ident ($($key:tt)*) (: $($unexpected:tt)+) $copy:tt) => {
        $crate::js_json_expect_expr_comma!($($unexpected)+);
    };
    (@object $object:ident ($($key:tt)*) ($tt:tt $($rest:tt)*) $copy:tt) => {
        $crate::js_json_internal!(@object $object ($($key)* $tt) ($($rest)*) ($($rest)*));
    };
    (null) => {
        $crate::js_json::JsValue::Null
    };
    (true) => {
        $crate::js_json::JsValue::Bool(true)
    };
    (false) => {
        $crate::js_json::JsValue::Bool(false)
    };
    ([]) => {
        $crate::js_json::JsValue::Array(::std::vec![])
    };
    ([ $($tt:tt)+ ]) => {
        $crate::js_json::JsValue::Array($crate::js_json_internal!(@array [] $($tt)+))
    };
    ({}) => {
        $crate::js_json::JsValue::Object($crate::js_json::JsObject::new())
    };
    ({ $($tt:tt)+ }) => {
        $crate::js_json::JsValue::Object({
            let mut object = $crate::js_json::JsObject::new();
            $crate::js_json_internal!(@object object () ($($tt)+) ($($tt)+));
            object
        })
    };
    ($other:expr) => {
        $crate::js_json::to_value(&$other).unwrap()
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! js_json_unexpected {
    () => {};
}
#[macro_export]
#[doc(hidden)]
macro_rules! js_json_expect_expr_comma {
    ($e:expr , $($tt:tt)*) => {};
}
