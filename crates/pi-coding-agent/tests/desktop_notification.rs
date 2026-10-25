use pi_js_compat::js_json::{JsString, JsValue, parse_lossless, stringify_lossless};
#[path = "support/env_probe.rs"]
mod env_probe;
use crate::shared_executable_script as executable_script;

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use pi_coding_agent::core::desktop_notification::{
    DEFAULT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS, DesktopNotification, DesktopNotificationUrgency,
    NEVER_EXPIRE_DESKTOP_NOTIFICATION_MS, PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS, send_desktop_notification,
};

const PREFIX: &str = "DESKTOP_NOTIFICATION_PROBE:";

fn write_script(path: &Path, contents: &str) {
    executable_script::write_executable_script(path, contents);
}

fn probe(notification: DesktopNotification, close_immediately: bool) -> (Vec<String>, Option<Vec<String>>) {
    let directory = tempfile::tempdir().unwrap();
    let bin = directory.path().join("bin");
    fs::create_dir(&bin).unwrap();
    write_script(
        &bin.join("notify-send"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$PI_NOTIFY_LOG\"\nprintf ' 42\\n'\n",
    );
    write_script(&bin.join("setsid"), "#!/bin/sh\nexec \"$@\"\n");
    write_script(
        &bin.join("busctl"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$PI_NOTIFY_LOG.busctl\"\n",
    );
    let log = directory.path().join("notify.log");
    let env = vec![
        ("PATH", OsString::from(bin.to_string_lossy().into_owned())),
        ("PI_NOTIFY_LOG", OsString::from(log.to_string_lossy().into_owned())),
        (
            "PI_CLOSE_IMMEDIATELY",
            OsString::from(if close_immediately { "1" } else { "0" }),
        ),
        (
            "PI_NOTIFY_BODY",
            OsString::from(stringify_lossless(&JsValue::String(notification.body))),
        ),
        (
            "PI_NOTIFY_TITLE",
            OsString::from(stringify_lossless(&JsValue::String(notification.title))),
        ),
        (
            "PI_NOTIFY_EXPIRE",
            OsString::from(notification.expire_time_ms.unwrap_or(i64::MIN).to_string()),
        ),
        (
            "PI_NOTIFY_URGENCY",
            OsString::from(notification.urgency.map_or("none", |urgency| urgency.as_str())),
        ),
    ];
    let output = env_probe::run_probe("desktop_notification_probe_child", &env);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split_once(PREFIX).map(|(_, rest)| rest))
        .collect();
    assert_eq!(lines.len(), 1, "probe must print one line: {stdout}");
    let payload: serde_json::Value = serde_json::from_str(lines[0]).expect("probe payload must be JSON");
    let notify = payload["notify"].as_str().unwrap().lines().map(str::to_owned).collect();
    let busctl = payload["busctl"]
        .as_str()
        .filter(|log| !log.is_empty())
        .map(|log| log.lines().map(str::to_owned).collect());
    (notify, busctl)
}

#[test]
fn exposes_the_source_constants() {
    assert_eq!(DEFAULT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS, -1);
    assert_eq!(NEVER_EXPIRE_DESKTOP_NOTIFICATION_MS, 0);
    assert_eq!(PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS, 999_999);
    assert_eq!(DesktopNotificationUrgency::Critical.as_str(), "critical");
    assert_eq!(DesktopNotificationUrgency::Low.as_str(), "low");
    assert_eq!(DesktopNotificationUrgency::Normal.as_str(), "normal");
}

#[test]
fn uses_the_spec_default_expiration_and_critical_urgency() {
    let (notify, _) = probe(
        DesktopNotification {
            body: "Body".into(),
            expire_time_ms: None,
            title: "Title".into(),
            urgency: None,
        },
        false,
    );
    assert_eq!(
        notify,
        vec![
            "--print-id",
            "--app-name=Pi",
            "--expire-time=-1",
            "--urgency=critical",
            "Title",
            "Body"
        ]
    );
}

#[test]
fn preserves_explicit_non_expiring_notifications_and_urgency() {
    let (notify, busctl) = probe(
        DesktopNotification {
            body: "Body".into(),
            expire_time_ms: Some(NEVER_EXPIRE_DESKTOP_NOTIFICATION_MS),
            title: "Title".into(),
            urgency: Some(DesktopNotificationUrgency::Low),
        },
        true,
    );
    assert_eq!(notify[2], "--expire-time=0");
    assert_eq!(notify[3], "--urgency=low");
    assert_eq!(
        busctl,
        Some(vec![
            "--user".to_owned(),
            "call".to_owned(),
            "org.freedesktop.Notifications".to_owned(),
            "/org/freedesktop/Notifications".to_owned(),
            "org.freedesktop.Notifications".to_owned(),
            "CloseNotification".to_owned(),
            "u".to_owned(),
            "42".to_owned()
        ])
    );
}

#[test]
fn defers_close_until_the_notification_id_resolves() {
    let (_, busctl) = probe(
        DesktopNotification {
            body: "Body".into(),
            expire_time_ms: Some(PERSISTENT_DESKTOP_NOTIFICATION_EXPIRE_TIME_MS),
            title: "Title".into(),
            urgency: None,
        },
        true,
    );
    let busctl = busctl.expect("close must run after the id resolves");
    assert_eq!(busctl.last().map(String::as_str), Some("42"));
}

#[test]
#[ignore = "child process probe"]
fn desktop_notification_probe_child() {
    if !env_probe::in_probe() {
        return;
    }
    let expire = std::env::var("PI_NOTIFY_EXPIRE").unwrap();
    let urgency = std::env::var("PI_NOTIFY_URGENCY").unwrap();
    let notification = DesktopNotification {
        body: parse_lossless(std::env::var("PI_NOTIFY_BODY").unwrap())
            .unwrap()
            .as_string()
            .unwrap()
            .clone(),
        expire_time_ms: (expire != i64::MIN.to_string()).then(|| expire.parse().unwrap()),
        title: parse_lossless(std::env::var("PI_NOTIFY_TITLE").unwrap())
            .unwrap()
            .as_string()
            .unwrap()
            .clone(),
        urgency: match urgency.as_str() {
            "critical" => Some(DesktopNotificationUrgency::Critical),
            "low" => Some(DesktopNotificationUrgency::Low),
            "normal" => Some(DesktopNotificationUrgency::Normal),
            _ => None,
        },
    };
    let handle = send_desktop_notification(&notification).expect("linux notifications must return a handle");
    let close_immediately = std::env::var("PI_CLOSE_IMMEDIATELY").unwrap() == "1";
    if close_immediately {
        handle.close().expect("close fixture notification");
    }
    let log = std::env::var("PI_NOTIFY_LOG").unwrap();
    let busctl_log = format!("{log}.busctl");
    let close_immediately = std::env::var("PI_CLOSE_IMMEDIATELY").unwrap() == "1";
    wait_for_file(Path::new(&log), close_immediately.then_some(Path::new(&busctl_log)));
    let notify = fs::read_to_string(&log).unwrap_or_default();
    let busctl = fs::read_to_string(&busctl_log).unwrap_or_default();
    println!("{PREFIX}{}", serde_json::json!({"notify": notify, "busctl": busctl}));
}

fn wait_for_file(log: &Path, also: Option<&Path>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let log_ready = log.exists();
        let also_ready = also.is_none_or(|path| path.exists());
        if log_ready && also_ready {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn notification_encodes_raw_title_and_body_only_at_notify_send_argv() {
    let raw = JsString::from_utf16(vec![0xd800]);
    let (argv, _) = probe(
        DesktopNotification {
            title: raw.clone(),
            body: raw,
            expire_time_ms: None,
            urgency: None,
        },
        false,
    );
    assert_eq!(&argv[4..], &["�", "�"]);
}

// Shared support is compiled once per integration target.
#[path = "support/executable_script.rs"]
pub mod shared_executable_script;
