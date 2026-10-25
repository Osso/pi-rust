use crate::shared_oracle as oracle;

use pi_coding_agent::{config::ConfigError, utils::paths::normalize_path};
use pi_js_compat::node_error::NativeNodeError;
use serde_json::{Value, json};
use std::error::Error;

#[test]
fn path_to_config_error_preserves_frozen_native_error_values_and_sources() {
    let paths = ["file://not-local/pkg", "file:///x%2fy", "file:///bad%ZZ", "file://["];
    let mut oracle = oracle::TsOracle::spawn(
        "path-native-error-values-oracle.mjs",
        "packages/coding-agent/src/utils/paths.ts",
    );
    let expected: Value = oracle.request(&json!({"paths":paths}));
    for (path, expected) in paths.iter().zip(expected.as_array().unwrap()) {
        let path_error = normalize_path(*path, &Default::default()).unwrap_err();
        let original = path_error.native_error().unwrap().clone();
        assert_eq!(
            path_error.source().unwrap().downcast_ref::<NativeNodeError>(),
            Some(&original)
        );
        let error = ConfigError::from(path_error);
        let native = error.native_error().unwrap();
        assert_eq!(native, &original);
        assert_eq!(error.source().unwrap().downcast_ref::<NativeNodeError>(), Some(native));
        let actual = json!({"error":native.value().to_serde().unwrap(),"constructor":native.constructor_class().name(),"text":native.string_value()});
        assert_eq!(&actual, expected, "{path}");
    }
}

#[test]
fn config_error_preserves_the_supplied_system_error_source() {
    let original = pi_js_compat::node_os::homedir_system_error(-5, "EIO", "i/o error");
    let error = ConfigError::from(original.clone());
    assert_eq!(error.native_error(), Some(&original));
    assert_eq!(
        error.source().unwrap().downcast_ref::<NativeNodeError>(),
        Some(&original)
    );
    assert_eq!(error.label(), "SystemError");
}
