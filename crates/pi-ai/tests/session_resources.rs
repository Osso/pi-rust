use std::sync::{Arc, Mutex};

use pi_ai::session_resources::{SessionResourceCleanup, cleanup_session_resources, register_session_resource_cleanup};

fn recorder(
    marker: &'static str,
    calls: Arc<Mutex<Vec<Option<pi_js_compat::js_json::JsString>>>>,
) -> SessionResourceCleanup {
    Box::new(move |session_id| {
        if session_id == Some(&pi_js_compat::js_json::JsString::from(marker)) {
            calls.lock().unwrap().push(session_id.map(str::to_owned));
        }
        Ok(())
    })
}

#[test]
fn invokes_registered_cleanups_with_the_session_id() {
    let marker = "session-resources-records";
    let calls = Arc::new(Mutex::new(Vec::new()));
    let _first = register_session_resource_cleanup(recorder(marker, Arc::clone(&calls)));
    let _second = register_session_resource_cleanup(recorder(marker, Arc::clone(&calls)));

    cleanup_session_resources(Some(&pi_js_compat::js_json::JsString::from(marker))).unwrap();

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[Some(marker.to_owned()), Some(marker.to_owned())]
    );
}

#[test]
fn unregister_stops_invocations() {
    let marker = "session-resources-unregister";
    let calls = Arc::new(Mutex::new(Vec::new()));
    let registration = register_session_resource_cleanup(recorder(marker, Arc::clone(&calls)));
    registration.unregister();

    cleanup_session_resources(Some(&pi_js_compat::js_json::JsString::from(marker))).unwrap();

    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn passes_a_missing_session_id_as_none() {
    let observed: Arc<Mutex<Vec<Option<pi_js_compat::js_json::JsString>>>> = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::clone(&observed);
    // The registry is process-global; parallel tests call it with their own markers.
    let _registration = register_session_resource_cleanup(Box::new(move |session_id| {
        if session_id.is_none() {
            calls.lock().unwrap().push(None);
        }
        Ok(())
    }));

    cleanup_session_resources(None).unwrap();

    assert_eq!(observed.lock().unwrap().as_slice(), &[None]);
}

#[test]
fn aggregates_cleanup_errors() {
    let marker = "session-resources-errors";
    let _first = register_session_resource_cleanup(Box::new(|session_id| {
        if session_id == Some(&pi_js_compat::js_json::JsString::from(marker)) {
            return Err("first failure".into());
        }
        Ok(())
    }));
    let _second = register_session_resource_cleanup(Box::new(|session_id| {
        if session_id == Some(&pi_js_compat::js_json::JsString::from(marker)) {
            return Err("second failure".into());
        }
        Ok(())
    }));

    let error = cleanup_session_resources(Some(&pi_js_compat::js_json::JsString::from(marker)))
        .expect_err("errors must aggregate");

    assert_eq!(error.to_string(), "Failed to cleanup session resources");
    assert_eq!(error.errors.len(), 2);
    assert_eq!(error.errors[0].to_string(), "first failure");
    assert_eq!(error.errors[1].to_string(), "second failure");
}
