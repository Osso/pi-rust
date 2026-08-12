use pi_protocol::{DEFAULT_MAX_FRAME_LENGTH, FrameEncodeError, FrameError, assert_complete_frame, encode_frame};

#[test]
fn prefixes_payload_with_unsigned_big_endian_length() {
    let frame = encode_frame(&[0xaa, 0xbb, 0xcc]).expect("three-byte payload must fit");

    assert_eq!(frame, [0x00, 0x00, 0x00, 0x03, 0xaa, 0xbb, 0xcc]);
    assert_eq!(encode_frame(&[]).expect("empty payload must fit"), [0, 0, 0, 0]);
}

#[test]
fn writes_nonzero_high_order_length_bytes() {
    let payload = vec![0x5a; 70_000];
    let frame = encode_frame(&payload).expect("70,000-byte payload must fit");

    assert_eq!(&frame[..4], &[0x00, 0x01, 0x11, 0x70]);
    assert_eq!(&frame[4..], payload);
}

#[test]
fn decodes_nonzero_high_order_length_bytes() {
    let payload = vec![0x5a; 70_000];
    let frame = encode_frame(&payload).expect("70,000-byte payload must fit");

    assert_eq!(assert_complete_frame(&frame, Some(70_000)), Ok(()));
}

#[test]
fn returns_owned_frame_bytes() {
    let mut payload = vec![1, 2, 3];
    let frame = encode_frame(&payload).expect("payload must fit");
    payload.fill(9);

    assert_eq!(frame, [0, 0, 0, 3, 1, 2, 3]);
}

#[test]
fn validates_exactly_one_complete_bounded_frame() {
    assert_eq!(assert_complete_frame(&[0, 0, 0, 2, 1, 2], Some(2)), Ok(()));
}

#[test]
fn uses_the_default_maximum_frame_length() {
    let declared_above_default = DEFAULT_MAX_FRAME_LENGTH + 1;
    let frame = declared_above_default.to_be_bytes();

    assert_eq!(
        assert_complete_frame(&frame, None),
        Err(FrameError::new(format!(
            "Frame length {declared_above_default} exceeds configured limit of {DEFAULT_MAX_FRAME_LENGTH}"
        )))
    );
}

#[test]
fn rejects_incomplete_length_prefix() {
    assert_eq!(
        assert_complete_frame(&[0, 0, 0], None),
        Err(FrameError::new("Frame does not contain a complete length prefix"))
    );
}

#[test]
fn rejects_declared_length_above_configured_limit() {
    assert_eq!(
        assert_complete_frame(&[0, 0, 0, 3, 1, 2, 3], Some(2)),
        Err(FrameError::new("Frame length 3 exceeds configured limit of 2"))
    );
}

#[test]
fn accepts_the_unsigned_32_bit_maximum_as_a_limit() {
    assert_eq!(
        assert_complete_frame(&[0xff, 0xff, 0xff, 0xff], Some(u32::MAX)),
        Err(FrameError::new("Frame must contain exactly one complete payload"))
    );
}

#[test]
fn rejects_partial_payload_and_trailing_bytes() {
    let expected = Err(FrameError::new("Frame must contain exactly one complete payload"));

    assert_eq!(assert_complete_frame(&[0, 0, 0, 2, 1], None), expected);
    assert_eq!(assert_complete_frame(&[0, 0, 0, 1, 1, 2], None), expected);
}

#[test]
fn frame_errors_preserve_source_messages() {
    let error = FrameError::new("message");

    assert_eq!(error.to_string(), "message");
    assert_eq!(
        FrameEncodeError::PayloadTooLarge.to_string(),
        "Frame payload exceeds the unsigned 32-bit length limit"
    );
}
