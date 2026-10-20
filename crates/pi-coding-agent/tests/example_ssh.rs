#[path = "../examples/extensions/ssh.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::core::extensions::*;
use std::sync::Arc;

#[tokio::test]
async fn ssh_fixture_remote_matches_pinned_example() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::ssh_extension(api);
        Box::pin(async move { result })
    });
    let runtime = create_extension_runtime();
    let extension = load_extension_from_factory(factory, "/fixture", support::event_bus(), runtime, "<example:ssh>")
        .await
        .unwrap();
    assert_eq!(extension.read().tools.len(), 4);
}
