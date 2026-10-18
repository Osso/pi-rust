use std::time::Duration;

use pi_protocol::{
    ProtocolError, ProtocolErrorCode, ServerHelloError, ServerMessage, ServerMessageDecoder, encode_server_message,
};
use pi_server::connection::ByteConnection;
use pi_server::transports::unix::UnixByteConnection;
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;

#[tokio::test]
async fn queues_a_final_protocol_error_behind_pending_output_before_closing() {
    let (mut client_stream, server_stream) = UnixStream::pair().expect("socket pair must be created");
    let (connection, _server_reader) = UnixByteConnection::new(server_stream, 1_000, 4 * 1024 * 1024);

    let pending_chunk = vec![7_u8; 1024 * 1024];
    let pending = {
        let connection = connection.clone();
        let chunk = pending_chunk.clone();
        tokio::spawn(async move { connection.send(chunk).await })
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while connection.pending_bytes() != pending_chunk.len() {
        assert!(tokio::time::Instant::now() < deadline, "pending send must be queued");
        tokio::task::yield_now().await;
    }

    let final_message = ServerMessage::HelloError(ServerHelloError::new(ProtocolError::new(
        ProtocolErrorCode::InvalidRequest,
        "Protocol violation".into(),
        None,
    )));
    let final_frame = encode_server_message(&final_message, None).expect("final frame must encode");
    let closing = {
        let connection = connection.clone();
        let frame = final_frame.clone();
        tokio::spawn(async move { connection.close(Some(frame)).await })
    };

    let mut received = Vec::new();
    client_stream
        .read_to_end(&mut received)
        .await
        .expect("stream must end after the final frame");
    pending.await.expect("send task must join").expect("send must succeed");
    connection.mark_closed();
    closing
        .await
        .expect("close task must join")
        .expect("close must succeed");

    assert_eq!(&received[..pending_chunk.len()], pending_chunk.as_slice());
    let messages = ServerMessageDecoder::new(None)
        .push(&received[pending_chunk.len()..])
        .expect("final frame must decode");
    assert_eq!(messages, vec![final_message]);
}
