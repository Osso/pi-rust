use super::{FrameDecodeError, FrameDecoder, FrameError};

#[test]
fn allocation_failure_is_terminal() {
    let mut decoder = FrameDecoder::with_forced_allocation_failure(Some(1));

    assert_eq!(
        FrameDecodeError::Allocation.to_string(),
        "Frame decoder allocation failed"
    );
    assert_eq!(decoder.push(&[0, 0, 0, 1, 7]), Err(FrameDecodeError::Allocation));
    assert_eq!(
        decoder.push(&[]),
        Err(FrameDecodeError::Frame(FrameError::new("Frame decoder has failed")))
    );
    assert_eq!(decoder.end(), Err(FrameError::new("Frame decoder has failed")));
}
