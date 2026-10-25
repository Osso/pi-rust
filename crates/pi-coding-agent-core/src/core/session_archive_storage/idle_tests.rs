//! Native real-filesystem/control-DB boundary proofs for frozen :167-183 and :225-240.
use super::*;

const SESSION: &str = "{\"type\":\"session\",\"version\":3,\"id\":\"boundary\",\"cwd\":\"/fixture\",\"timestamp\":\"2020-01-01T00:00:00.000Z\"}\n";

fn create_fixture(root: &Path) -> (JsString, JsString) {
    let path = JsString::from(root.join("case.jsonl").to_str().unwrap());
    let db = JsString::from(root.join("control.db").to_str().unwrap());
    fs::write(encode_node_utf8(&path), SESSION).unwrap();
    assert!(index_transcript(&db, &path).unwrap());
    (db, path)
}

#[test]
fn publication_rechecks_real_file_identity_and_live_reference_after_snapshot() {
    for mutation in ["unchanged", "open", "append", "rewrite", "replace", "mtime", "removed"] {
        let root = tempfile::tempdir().unwrap();
        let (db, path) = create_fixture(root.path());
        let disk_path = encode_node_utf8(&path);
        let before = fs::symlink_metadata(&disk_path).unwrap();
        let mut descriptor = None;
        match mutation {
            "unchanged" => {}
            "open" => descriptor = Some(File::open(&disk_path).unwrap()),
            "append" => {
                use std::io::Write;
                OpenOptions::new()
                    .append(true)
                    .open(&disk_path)
                    .unwrap()
                    .write_all(b"append")
                    .unwrap();
            }
            "rewrite" => fs::write(&disk_path, SESSION.replace("fixture", "changed")).unwrap(),
            "replace" => {
                fs::rename(&disk_path, root.path().join("old-inode")).unwrap();
                fs::write(&disk_path, SESSION).unwrap();
            }
            "mtime" => File::open(&disk_path)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH)
                .unwrap(),
            "removed" => fs::remove_file(&disk_path).unwrap(),
            _ => unreachable!(),
        }
        let outcome = can_publish(&db, &path, &before);
        if mutation == "removed" {
            let error = outcome.unwrap_err().thrown_value().value().to_serde().unwrap();
            assert_eq!(error["code"], "ENOENT");
            assert_eq!(error["syscall"], "realpath");
            assert_eq!(error["path"], disk_path);
        } else {
            assert_eq!(outcome.unwrap(), mutation == "unchanged", "{mutation}");
        }
        drop(descriptor);
    }
}

#[test]
fn publication_requeries_real_database_guards_added_after_snapshot() {
    for guard in ["health", "listener", "owner", "child"] {
        let root = tempfile::tempdir().unwrap();
        let (db, path) = create_fixture(root.path());
        let before = fs::symlink_metadata(encode_node_utf8(&path)).unwrap();
        let connection = Connection::open(encode_node_utf8(&db)).unwrap();
        match guard {
            "health" => connection.execute("INSERT INTO session_health(session_id,agent_generation,pid,check_status,updated_at) VALUES('boundary',1,?,'ok','fixture')", [std::process::id()]).unwrap(),
            "listener" => connection.execute("INSERT INTO runtime_mailbox_listeners(recipient_session_id,recipient_agent_id_key,pid,session_path,session_path_asserted_at,updated_at) VALUES('boundary','',?,?,'fixture','fixture')", rusqlite::params![std::process::id(), path]).unwrap(),
            "owner" => connection.execute("INSERT INTO multi_agent_runtime_owners(session_path,agent_id,process_identity) VALUES(?,'child','live')", [&path]).unwrap(),
            "child" => connection.execute("INSERT INTO multi_agent_agents(session_path,agent_id,data,updated_at) VALUES(?,'child','{}','fixture')", [&path]).unwrap(),
            _ => unreachable!(),
        };
        drop(connection);
        assert!(!can_publish(&db, &path, &before).unwrap(), "{guard}");
    }
}

#[test]
fn hard_link_publication_collision_preserves_both_files_and_metadata() {
    let root = tempfile::tempdir().unwrap();
    let (db, path) = create_fixture(root.path());
    let temp = path.clone() + ".archive-temp-fixture";
    let target = path.clone() + ".zst";
    fs::write(encode_node_utf8(&temp), b"new archive").unwrap();
    fs::write(encode_node_utf8(&target), b"other pass").unwrap();
    let error = publish_archive(&db, &path, &temp).unwrap_err();
    assert_eq!(
        error.message,
        pi_js_compat::js_format!("EEXIST: file already exists, link '{}' -> '{}'", temp, target)
    );
    assert_eq!(fs::read(encode_node_utf8(&target)).unwrap(), b"other pass");
    assert_eq!(fs::read(encode_node_utf8(&path)).unwrap(), SESSION.as_bytes());
    assert_eq!(
        read_session_metadata(&db, &path).unwrap().unwrap().is_archived,
        Some(false)
    );
}

#[test]
fn unlink_failure_unarchives_metadata_and_removes_published_target() {
    let root = tempfile::tempdir().unwrap();
    let (db, path) = create_fixture(root.path());
    let temp = path.clone() + ".archive-temp-fixture";
    let target = path.clone() + ".zst";
    fs::write(encode_node_utf8(&temp), b"archive").unwrap();
    fs::remove_file(encode_node_utf8(&path)).unwrap();
    fs::create_dir(encode_node_utf8(&path)).unwrap();
    let error = publish_archive(&db, &path, &temp).unwrap_err();
    assert_eq!(
        error.message,
        pi_js_compat::js_format!("EISDIR: illegal operation on a directory, unlink '{}'", path)
    );
    assert!(!exists(&target));
    assert!(exists(&temp));
    assert_eq!(
        read_session_metadata(&db, &path).unwrap().unwrap().is_archived,
        Some(false)
    );
    assert!(read_session_metadata(&db, &target).unwrap().is_none());
}
