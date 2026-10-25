use crate::shared_image_fixtures as image_fixtures;

use pi_coding_agent::utils::exif_orientation::{apply_exif_orientation, read_exif_orientation};
use pi_coding_agent::utils::photon::decode_image;

const ORIGINAL_PIXELS: [[u8; 4]; 4] = [
    [10, 20, 30, 255],
    [40, 50, 60, 255],
    [70, 80, 90, 255],
    [100, 110, 120, 255],
];

/// 2x2 JPEG with four distinct pixels, decoded to RGBA.
fn distinct_jpeg() -> Vec<u8> {
    let pixels = ORIGINAL_PIXELS.concat();
    let image = image::RgbaImage::from_raw(2, 2, pixels).expect("2x2 pixel buffer");
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageOutputFormat::Jpeg(100),
        )
        .expect("fixture JPEG must encode");
    jpeg
}

fn jpeg_with_orientation(orientation: u16, little_endian: bool) -> Vec<u8> {
    let mut jpeg = distinct_jpeg();
    jpeg.splice(2..2, app1_segment(&tiff_with_orientation(orientation, little_endian)));
    jpeg
}

fn app1_segment(tiff: &[u8]) -> Vec<u8> {
    let mut segment = vec![0xff, 0xe1];
    segment.extend_from_slice(&(2u16 + 6 + tiff.len() as u16).to_be_bytes());
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(tiff);
    segment
}

fn tiff_with_orientation(orientation: u16, little_endian: bool) -> Vec<u8> {
    let mut tiff = Vec::new();
    let push16 = |tiff: &mut Vec<u8>, value: u16| {
        tiff.extend_from_slice(&if little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    };
    let push32 = |tiff: &mut Vec<u8>, value: u32| {
        tiff.extend_from_slice(&if little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    };

    tiff.extend_from_slice(if little_endian { b"II" } else { b"MM" });
    push16(&mut tiff, 42);
    push32(&mut tiff, 8);
    push16(&mut tiff, 1);
    push16(&mut tiff, 0x0112);
    push16(&mut tiff, 3);
    push32(&mut tiff, 1);
    push16(&mut tiff, orientation);
    push16(&mut tiff, 0);
    push32(&mut tiff, 0);
    tiff
}

fn webp_with_exif_chunk(tiff: &[u8], prefixed: bool) -> Vec<u8> {
    let mut chunk = Vec::new();
    if prefixed {
        chunk.extend_from_slice(b"Exif\0\0");
    }
    chunk.extend_from_slice(tiff);

    let mut webp = Vec::new();
    webp.extend_from_slice(b"RIFF");
    webp.extend_from_slice(&0u32.to_le_bytes());
    webp.extend_from_slice(b"WEBP");
    webp.extend_from_slice(b"EXIF");
    webp.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
    webp.extend_from_slice(&chunk);
    if chunk.len() % 2 == 1 {
        webp.push(0);
    }
    webp
}

#[test]
fn reads_little_and_big_endian_jpeg_orientations() {
    for orientation in 1..=8u16 {
        for little_endian in [true, false] {
            let bytes = jpeg_with_orientation(orientation, little_endian);
            assert_eq!(read_exif_orientation(&bytes), orientation as u8);
        }
    }
}

#[test]
fn reads_webp_exif_chunks_with_and_without_the_exif_prefix() {
    for orientation in 1..=8u16 {
        for prefixed in [true, false] {
            let webp = webp_with_exif_chunk(&tiff_with_orientation(orientation, true), prefixed);
            assert_eq!(read_exif_orientation(&webp), orientation as u8, "{prefixed}");
        }
    }
}

/// Frozen 4f9e6f8a4 includes unsigned chunk/TIFF reads and bounded scans.
mod upstream_chunk_bounds {
    use pi_coding_agent::utils::exif_orientation::read_exif_orientation;

    const ORIGINAL: [u8; 8] = [255, 0, 0, 255, 0, 0, 255, 255];
    const FLIPPED: [u8; 8] = [0, 0, 255, 255, 255, 0, 0, 255];

    #[test]
    fn terminates_on_huge_webp_chunk_sizes() {
        for size in [0xffff_fff8u32, 0xffff_fff7, 0xffff_fff6] {
            let bytes = webp(&[webp_chunk_with_size(b"JUNK", &[], size)]);
            assert_eq!(read_exif_orientation(&bytes), 1, "{size:#x}");
            assert_eq!(apply(&bytes), ORIGINAL, "{size:#x}");
        }
    }

    #[test]
    fn rejects_oversized_unsigned_exif_chunk_sizes() {
        for size in [0x8000_0000u32, 0xffff_fff8, 0xffff_ffff] {
            let bytes = webp(&[webp_chunk_with_size(b"EXIF", &TIFF_MIRROR_HORIZONTAL, size)]);
            assert_eq!(read_exif_orientation(&bytes), 1, "{size:#x}");
            assert_eq!(apply(&bytes), ORIGINAL, "{size:#x}");
        }
    }

    #[test]
    fn rejects_a_chunk_whose_required_odd_byte_padding_exceeds_the_buffer() {
        let mut unpadded = TIFF_MIRROR_HORIZONTAL.to_vec();
        unpadded.push(0);
        assert_eq!(apply(&webp(&[webp_chunk(b"EXIF", &unpadded)])), ORIGINAL);
    }

    #[test]
    fn applies_valid_webp_orientation_with_or_without_the_exif_prefix() {
        let mut prefixed = b"Exif\0\0".to_vec();
        prefixed.extend_from_slice(&TIFF_MIRROR_HORIZONTAL);
        for exif in [TIFF_MIRROR_HORIZONTAL.to_vec(), prefixed] {
            assert_eq!(apply(&webp(&[webp_chunk(b"EXIF", &exif)])), FLIPPED);
        }
    }

    #[test]
    fn advances_over_empty_and_padded_chunks() {
        let bytes = webp(&[
            webp_chunk(b"JUNK", &[]),
            webp_chunk_with_size(b"VP8 ", &[0], 1),
            vec![0],
            webp_chunk(b"EXIF", &TIFF_MIRROR_HORIZONTAL),
        ]);
        assert_eq!(apply(&bytes), FLIPPED);
    }

    #[test]
    fn rejects_a_truncated_webp_chunk() {
        let size = TIFF_MIRROR_HORIZONTAL.len() as u32 + 2;
        assert_eq!(
            apply(&webp(&[webp_chunk_with_size(b"EXIF", &TIFF_MIRROR_HORIZONTAL, size)])),
            ORIGINAL
        );
    }

    #[test]
    fn applies_jpeg_orientation_after_fill_markers_and_an_empty_segment() {
        let mut exif = b"Exif\0\0".to_vec();
        exif.extend_from_slice(&TIFF_MIRROR_HORIZONTAL);
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xff, 0xe0, 0, 2];
        bytes.push(0xff);
        bytes.push(0xe1);
        bytes.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        bytes.extend_from_slice(&exif);
        assert_eq!(apply(&bytes), FLIPPED);
    }

    #[test]
    fn rejects_a_truncated_jpeg_segment() {
        assert_eq!(apply(&[0xff, 0xd8, 0xff, 0xe0, 0xff, 0xff]), ORIGINAL);
    }

    #[test]
    fn terminates_on_an_invalid_signed_tiff_ifd_offset() {
        let invalid_tiff = [0x49, 0x49, 0x2a, 0x00, 0xf8, 0xff, 0xff, 0xff];
        assert_eq!(apply(&webp(&[webp_chunk(b"EXIF", &invalid_tiff)])), ORIGINAL);
    }

    /// Little-endian TIFF with one SHORT orientation entry: mirror horizontally.
    const TIFF_MIRROR_HORIZONTAL: [u8; 26] = [
        0x49, 0x49, 0x2a, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00,
        0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    fn webp_chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        webp_chunk_with_size(id, data, data.len() as u32)
    }

    fn webp_chunk_with_size(id: &[u8; 4], data: &[u8], size: u32) -> Vec<u8> {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(id);
        chunk.extend_from_slice(&size.to_le_bytes());
        chunk.extend_from_slice(data);
        chunk
    }

    fn webp(chunks: &[Vec<u8>]) -> Vec<u8> {
        let body = chunks.concat();
        let mut webp = Vec::new();
        webp.extend_from_slice(b"RIFF");
        webp.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(&body);
        webp
    }

    /// Applies the orientation to a 2x1 red/blue image and returns its pixels.
    fn apply(bytes: &[u8]) -> Vec<u8> {
        let image = photon_rs::PhotonImage::new(ORIGINAL.to_vec(), 2, 1);
        super::apply_exif_orientation(image, bytes).get_raw_pixels()
    }
}

#[test]
fn defaults_to_orientation_one_without_usable_exif() {
    assert_eq!(read_exif_orientation(&[]), 1);
    assert_eq!(read_exif_orientation(&[0xff]), 1);
    assert_eq!(read_exif_orientation(b"GIF89a"), 1);
    assert_eq!(
        read_exif_orientation(&image_fixtures::decode(image_fixtures::TINY_JPEG)),
        1
    );

    for orientation in [0, 9, 0xffff] {
        assert_eq!(
            read_exif_orientation(&jpeg_with_orientation(orientation, true)),
            1,
            "{orientation}"
        );
    }
}

#[test]
fn defaults_to_orientation_one_for_truncated_segments() {
    let mut jpeg = jpeg_with_orientation(6, true);
    jpeg.truncate(20);
    assert_eq!(read_exif_orientation(&jpeg), 1);

    let mut webp = webp_with_exif_chunk(&tiff_with_orientation(6, true), false);
    webp.truncate(webp.len() - 4);
    assert_eq!(read_exif_orientation(&webp), 1);
}

/// Frozen 4f9e6f8a4 reads RIFF chunk sizes as unsigned and bounds the scan.
#[test]
fn negative_webp_chunk_sizes_do_not_hang() {
    let mut webp = Vec::new();
    webp.extend_from_slice(b"RIFF");
    webp.extend_from_slice(&0u32.to_le_bytes());
    webp.extend_from_slice(b"WEBP");
    webp.extend_from_slice(b"ICCP");
    webp.extend_from_slice(&(-8i32).to_le_bytes());
    webp.extend_from_slice(&[0; 16]);
    assert_eq!(read_exif_orientation(&webp), 1);
}

#[test]
fn applies_the_eight_orientation_transforms() {
    let image = decode_image(&distinct_jpeg()).unwrap();
    let original = image.get_raw_pixels();
    let distinct = (0..4)
        .map(|index| original[index * 4..index * 4 + 4].to_vec())
        .collect::<Vec<_>>();
    assert_eq!(
        distinct.iter().collect::<std::collections::HashSet<_>>().len(),
        4,
        "fixture pixels must stay distinct after JPEG encoding"
    );

    for orientation in 1..=8u8 {
        let applied = apply_exif_orientation(image.clone(), &jpeg_with_orientation(orientation as u16, true));
        assert_eq!(
            applied.get_raw_pixels(),
            expected_pixels(orientation, &original),
            "orientation {orientation}"
        );
    }
}

#[test]
fn swaps_dimensions_for_quarter_turns() {
    let jpeg = {
        let image = image::RgbaImage::from_fn(3, 2, |x, y| image::Rgba([x as u8 * 40, y as u8 * 80, 7, 255]));
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg),
                image::ImageOutputFormat::Jpeg(100),
            )
            .expect("fixture JPEG must encode");
        jpeg
    };
    let image = decode_image(&jpeg).unwrap();

    for (orientation, expected) in [(5u16, (2, 3)), (6, (2, 3)), (7, (2, 3)), (8, (2, 3))] {
        let mut bytes = jpeg.clone();
        bytes.splice(2..2, app1_segment(&tiff_with_orientation(orientation, true)));
        let applied = apply_exif_orientation(image.clone(), &bytes);
        assert_eq!((applied.get_width(), applied.get_height()), expected, "{orientation}");
    }
}

fn expected_pixels(orientation: u8, original: &[u8]) -> Vec<u8> {
    let width = 2u32;
    let height = 2u32;
    let (output_width, output_height) = match orientation {
        5..=8 => (height, width),
        _ => (width, height),
    };

    let mut output = Vec::new();
    for y in 0..output_height {
        for x in 0..output_width {
            let (source_x, source_y) = match orientation {
                1 => (x, y),
                2 => (width - 1 - x, y),
                3 => (width - 1 - x, height - 1 - y),
                4 => (x, height - 1 - y),
                5 => (y, x),
                6 => (y, height - 1 - x),
                7 => (width - 1 - y, height - 1 - x),
                _ => (width - 1 - y, x),
            };
            let source = ((source_y * width + source_x) * 4) as usize;
            output.extend_from_slice(&original[source..source + 4]);
        }
    }
    output
}
