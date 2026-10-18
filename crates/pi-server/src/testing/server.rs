use std::sync::Arc;

use crate::server::{PiServer, PiServerConfigError, PiServerOptions};
use crate::testing::service::TestServerService;
use crate::types::PiServerService;

pub struct TestServer {
    pub server: PiServer,
    pub service: Arc<dyn PiServerService>,
}

/// Creates an unstarted server with deterministic defaults for transport conformance tests.
pub fn create_test_server(options: PiServerOptions) -> Result<TestServer, PiServerConfigError> {
    let service: Arc<dyn PiServerService> = Arc::new(TestServerService::new());
    create_test_server_with_service(service, options)
}

pub fn create_test_server_with_service(
    service: Arc<dyn PiServerService>,
    options: PiServerOptions,
) -> Result<TestServer, PiServerConfigError> {
    Ok(TestServer {
        server: PiServer::new(service.clone(), options)?,
        service,
    })
}
