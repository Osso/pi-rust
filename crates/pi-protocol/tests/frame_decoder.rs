use pi_protocol::{FrameDecodeError, FrameDecoder, FrameError, encode_frame};

fn concatenate(chunks: &[Vec<u8>]) -> Vec<u8> {
    let total_length = chunks.iter().map(Vec::len).sum();
    let mut result = Vec::with_capacity(total_length);
    for chunk in chunks {
        result.extend_from_slice(chunk);
    }
    result
}

fn patterned_payload(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

fn framing_error(message: impl Into<String>) -> FrameDecodeError {
    FrameDecodeError::Frame(FrameError::new(message))
}

#[test]
fn accepts_empty_chunks_and_a_clean_empty_stream() {
    let mut decoder = FrameDecoder::new(None);

    assert_eq!(decoder.push(&[]), Ok(Vec::new()));
    assert_eq!(decoder.end(), Ok(()));
}

#[test]
fn decodes_fragmented_coalesced_and_empty_frames_in_order() {
    let wire = concatenate(&[
        encode_frame(&[1, 2, 3]).expect("payload must fit"),
        encode_frame(&[]).expect("payload must fit"),
        encode_frame(&[4]).expect("payload must fit"),
    ]);
    let mut fragmented = FrameDecoder::new(None);
    let mut fragmented_frames = Vec::new();
    for byte in &wire {
        fragmented_frames.extend(fragmented.push(&[*byte]).expect("byte push must decode"));
    }
    fragmented.end().expect("complete stream must end");

    assert_eq!(fragmented_frames, [vec![1, 2, 3], vec![], vec![4]]);

    let mut coalesced = FrameDecoder::new(None);
    assert_eq!(coalesced.push(&wire), Ok(vec![vec![1, 2, 3], vec![], vec![4]]));
    assert_eq!(coalesced.end(), Ok(()));
}

#[test]
fn handles_every_split_point_across_a_frame() {
    let expected_payload = vec![10, 20, 30, 40];
    let wire = encode_frame(&expected_payload).expect("payload must fit");

    for split in 0..=wire.len() {
        let mut decoder = FrameDecoder::new(None);
        let mut frames = decoder.push(&wire[..split]).expect("prefix must decode");
        frames.extend(decoder.push(&wire[split..]).expect("suffix must decode"));
        decoder.end().expect("complete stream must end");
        assert_eq!(
            frames.as_slice(),
            std::slice::from_ref(&expected_payload),
            "split {split}"
        );
    }
}

#[test]
fn preserves_single_and_multiple_block_boundaries() {
    for payload_length in [65_536, 65_537, 70_000] {
        let payload = patterned_payload(payload_length);
        let wire = encode_frame(&payload).expect("payload must fit");
        let mut decoder = FrameDecoder::new(None);
        let frames = [
            decoder.push(&wire[..101]).expect("first chunk must decode"),
            decoder
                .push(&wire[101..wire.len().min(65_541)])
                .expect("second chunk must decode"),
            decoder
                .push(&wire[wire.len().min(65_541)..])
                .expect("final chunk must decode"),
        ]
        .concat();
        decoder.end().expect("complete stream must end");

        assert_eq!(frames, [payload], "payload length {payload_length}");
    }
}

#[test]
fn returned_frames_are_independent_of_input_and_later_decoder_activity() {
    let mut first_wire = encode_frame(&[1, 2, 3]).expect("payload must fit");
    let mut second_wire = encode_frame(&[4, 5, 6]).expect("payload must fit");
    let mut decoder = FrameDecoder::new(None);

    let mut first_frames = decoder.push(&first_wire).expect("first frame must decode");
    first_wire.fill(9);
    let mut second_frames = decoder.push(&second_wire).expect("second frame must decode");
    second_wire.fill(8);
    first_frames[0][0] = 7;

    assert_eq!(first_frames, [vec![7, 2, 3]]);
    assert_eq!(second_frames, [vec![4, 5, 6]]);
    second_frames[0][0] = 6;
    assert_eq!(first_frames, [vec![7, 2, 3]]);
}

#[test]
fn rejects_truncated_headers_and_payloads_at_end() {
    for wire in [vec![0, 0, 0], vec![0, 0, 0, 2, 1]] {
        let mut decoder = FrameDecoder::new(None);
        assert_eq!(decoder.push(&wire), Ok(Vec::new()));
        assert_eq!(decoder.end(), Err(FrameError::new("Truncated frame at end of stream")));
        assert_eq!(decoder.end(), Err(FrameError::new("Frame decoder has failed")));
    }
}

#[test]
fn rejects_oversized_frames_immediately_and_remains_failed() {
    let mut decoder = FrameDecoder::new(Some(3));

    assert_eq!(
        decoder.push(&[0, 0, 0, 4]),
        Err(framing_error("Frame length 4 exceeds configured limit of 3"))
    );
    assert_eq!(decoder.push(&[1]), Err(framing_error("Frame decoder has failed")));
    assert_eq!(decoder.end(), Err(FrameError::new("Frame decoder has failed")));
}

#[test]
fn discards_frames_completed_before_a_later_same_push_failure() {
    let wire = concatenate(&[encode_frame(&[1]).expect("payload must fit"), vec![0, 0, 0, 4]]);
    let mut decoder = FrameDecoder::new(Some(3));

    assert_eq!(
        decoder.push(&wire),
        Err(framing_error("Frame length 4 exceeds configured limit of 3"))
    );
    assert_eq!(decoder.push(&[]), Err(framing_error("Frame decoder has failed")));
}

#[test]
fn accepts_a_frame_exactly_at_the_configured_maximum() {
    let mut decoder = FrameDecoder::new(Some(3));

    assert_eq!(decoder.push(&[0, 0, 0, 3, 1, 2, 3]), Ok(vec![vec![1, 2, 3]]));
    assert_eq!(decoder.end(), Ok(()));
}

#[test]
fn uses_the_default_maximum_frame_length() {
    let declared_length = pi_protocol::DEFAULT_MAX_FRAME_LENGTH + 1;
    let mut decoder = FrameDecoder::new(None);

    assert_eq!(
        decoder.push(&declared_length.to_be_bytes()),
        Err(framing_error(format!(
            "Frame length {declared_length} exceeds configured limit of {}",
            pi_protocol::DEFAULT_MAX_FRAME_LENGTH
        )))
    );
}

#[test]
fn ended_state_is_terminal() {
    let mut decoder = FrameDecoder::new(None);
    assert_eq!(decoder.end(), Ok(()));

    assert_eq!(decoder.push(&[]), Err(framing_error("Frame decoder has ended")));
    assert_eq!(decoder.end(), Err(FrameError::new("Frame decoder has ended")));
}
