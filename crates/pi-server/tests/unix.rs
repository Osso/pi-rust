use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::Arc;

use pi_server::PiServerService;
use pi_server::testing::{TestServerService, connect_unix_test_client};
use pi_server::transports::unix::{UnixServerOptions, create_unix_server};

fn service() -> Arc<dyn PiServerService> {
    Arc::new(TestServerService::new())
}

fn server_at(path: &str) -> pi_server::PiServer {
    create_unix_server(
        service(),
        UnixServerOptions {
            path: path.to_string(),
            ..UnixServerOptions::default()
        },
    )
    .expect("server options must be valid")
}

fn socket_path(directory: &tempfile::TempDir, nested: bool) -> String {
    if nested {
        directory
            .path()
            .join("p")
            .join("n")
            .join("server.sock")
            .to_string_lossy()
            .into_owned()
    } else {
        directory.path().join("server.sock").to_string_lossy().into_owned()
    }
}

#[tokio::test]
async fn rejects_a_live_listener_without_unlinking_it() {
    let directory = tempfile::tempdir().expect("temporary directory must be created");
    let path = socket_path(&directory, false);
    let first = server_at(&path);
    first.start().await.expect("first server must start");
    let first_identity = std::fs::symlink_metadata(&path).expect("socket must exist");

    let second = server_at(&path);
    let error = second.start().await.expect_err("second server must fail");
    assert!(error.to_string().contains("already running"));
    let current_identity = std::fs::symlink_metadata(&path).expect("socket must still exist");
    assert!(current_identity.file_type().is_socket());
    assert_eq!(current_identity.dev(), first_identity.dev());
    assert_eq!(current_identity.ino(), first_identity.ino());

    let client = connect_unix_test_client(&path).await.expect("client must connect");
    assert!(matches!(
        client.hello_default().await.expect("hello must succeed"),
        pi_protocol::ServerMessage::Hello(_)
    ));
    client.close().await;
    first.close().await.expect("first server must close");
}

#[tokio::test]
async fn never_unlinks_a_regular_file_at_the_configured_path() {
    let directory = tempfile::tempdir().expect("temporary directory must be created");
    let path = socket_path(&directory, false);
    std::fs::write(&path, "do not remove").expect("file must be written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).expect("file mode must be set");
    let server = server_at(&path);
    let error = server.start().await.expect_err("server must refuse the file");
    assert!(error.to_string().contains("non-socket"));
    assert_eq!(
        std::fs::read_to_string(&path).expect("file must remain readable"),
        "do not remove"
    );
}

#[tokio::test]
async fn creates_nested_temp_parents_restricts_permissions_and_removes_its_own_socket() {
    let directory = tempfile::tempdir().expect("temporary directory must be created");
    let path = socket_path(&directory, true);
    let server = server_at(&path);
    server.start().await.expect("server must start");
    let metadata = std::fs::symlink_metadata(&path).expect("socket must exist");
    assert!(metadata.file_type().is_socket());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);

    server.close().await.expect("server must close");
    assert!(std::fs::symlink_metadata(&path).is_err());
}

#[tokio::test]
async fn does_not_remove_a_replacement_inode_during_shutdown() {
    let directory = tempfile::tempdir().expect("temporary directory must be created");
    let path = socket_path(&directory, false);
    let server = server_at(&path);
    server.start().await.expect("server must start");
    std::fs::remove_file(&path).expect("socket must be removed");
    std::fs::write(&path, "replacement").expect("replacement must be written");

    server.close().await.expect("server must close");
    assert_eq!(
        std::fs::read_to_string(&path).expect("replacement must remain"),
        "replacement"
    );
}

#[tokio::test]
async fn removes_a_genuinely_stale_socket_before_binding() {
    let directory = tempfile::tempdir().expect("temporary directory must be created");
    let path = socket_path(&directory, false);
    let stale = tokio::net::UnixListener::bind(&path).expect("stale socket must bind");
    drop(stale);
    let stale_metadata = std::fs::symlink_metadata(&path).expect("stale socket must exist");
    assert!(stale_metadata.file_type().is_socket());

    let server = server_at(&path);
    server.start().await.expect("server must replace the stale socket");
    let live_metadata = std::fs::symlink_metadata(&path).expect("live socket must exist");
    assert!(live_metadata.file_type().is_socket());
    let client = connect_unix_test_client(&path).await.expect("client must connect");
    assert!(matches!(
        client.hello_default().await.expect("hello must succeed"),
        pi_protocol::ServerMessage::Hello(_)
    ));
    client.close().await;
    server.close().await.expect("server must close");
}
