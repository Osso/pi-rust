use pi_agent_core::harness::tools::image::detect_supported_image_mime_type;

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

#[test]
fn detects_short_signatures_and_case_sensitive_near_misses() {
    assert_eq!(detect_supported_image_mime_type(&[]), None);
    assert_eq!(detect_supported_image_mime_type(&[0xff, 0xd8]), None);
    assert_eq!(
        detect_supported_image_mime_type(&[0xff, 0xd8, 0xff]),
        Some("image/jpeg")
    );
    assert_eq!(detect_supported_image_mime_type(&[0xff, 0xd8, 0xff, 0xf7]), None);
    assert_eq!(detect_supported_image_mime_type(b"GIF"), Some("image/gif"));
    assert_eq!(detect_supported_image_mime_type(b"gif"), None);
    assert_eq!(
        detect_supported_image_mime_type(b"RIFF\0\0\0\0WEBP"),
        Some("image/webp")
    );
    assert_eq!(detect_supported_image_mime_type(b"RIFF\0\0\0\0webp"), None);
    assert_eq!(detect_supported_image_mime_type(b"BM"), None);
}

#[test]
fn accepts_minimal_png_and_preserves_apng_chunk_order_quirks() {
    assert_eq!(detect_supported_image_mime_type(&minimal_png()), Some("image/png"));
    assert_eq!(
        detect_supported_image_mime_type(&png_with_chunks(&[(b"acTL", 0)])),
        None
    );
    assert_eq!(
        detect_supported_image_mime_type(&png_with_chunks(&[(b"IDAT", 0), (b"acTL", 0)])),
        Some("image/png")
    );
    assert_eq!(
        detect_supported_image_mime_type(&png_with_chunks(&[(b"tEXt", u32::MAX)])),
        Some("image/png")
    );
}

#[test]
fn rejects_png_near_misses_before_chunk_scanning() {
    let mut wrong_length = minimal_png();
    write_u32_be(&mut wrong_length, 8, 12);
    assert_eq!(detect_supported_image_mime_type(&wrong_length), None);

    let mut wrong_type = minimal_png();
    wrong_type[12..16].copy_from_slice(b"IHDR");
    wrong_type[12] = b"i"[0];
    assert_eq!(detect_supported_image_mime_type(&wrong_type), None);

    let mut wrong_signature = minimal_png();
    wrong_signature[0] = 0x89 ^ 1;
    assert_eq!(detect_supported_image_mime_type(&wrong_signature), None);
}

#[test]
fn accepts_bmp_core_and_modern_boundary_variants() {
    for bits_per_pixel in [1, 4, 8, 16, 24, 32] {
        let bmp = bmp_dib12(0, 26, 1, bits_per_pixel);
        assert_eq!(detect_supported_image_mime_type(&bmp), Some("image/bmp"));
    }

    for (dib_header_size, pixel_offset) in [(40, 54), (124, 138)] {
        let bmp = bmp_dib40(0, pixel_offset, dib_header_size, 1, 24, 30);
        assert_eq!(detect_supported_image_mime_type(&bmp), Some("image/bmp"));
    }
}

#[test]
fn rejects_bmp_invalid_size_offset_header_planes_and_depth() {
    assert_eq!(detect_supported_image_mime_type(&bmp_dib12(25, 26, 1, 24)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_dib12(0, 25, 1, 24)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_dib12(26, 26, 1, 24)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_dib12(0, 26, 2, 24)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_dib12(0, 26, 1, 2)), None);
    assert_eq!(detect_supported_image_mime_type(&bmp_dib40(0, 54, 39, 1, 24, 30)), None);
    assert_eq!(
        detect_supported_image_mime_type(&bmp_dib40(0, 54, 125, 1, 24, 30)),
        None
    );
    assert_eq!(detect_supported_image_mime_type(&bmp_dib40(0, 54, 40, 1, 24, 26)), None);
}

fn minimal_png() -> Vec<u8> {
    let mut png = PNG_SIGNATURE.to_vec();
    png.extend_from_slice(&[0, 0, 0, 13]);
    png.extend_from_slice(b"IHDR");
    png
}

fn png_with_chunks(chunks: &[(&[u8; 4], u32)]) -> Vec<u8> {
    let mut png = PNG_SIGNATURE.to_vec();
    append_png_chunk(&mut png, b"IHDR", 13, 13);
    for (chunk_type, chunk_length) in chunks {
        append_png_chunk(&mut png, chunk_type, *chunk_length, 0);
    }
    png
}

fn append_png_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], chunk_length: u32, data_length: u32) {
    png.extend_from_slice(&chunk_length.to_be_bytes());
    png.extend_from_slice(chunk_type);
    png.extend(std::iter::repeat_n(0, data_length as usize));
    png.extend_from_slice(&[0, 0, 0, 0]);
}

fn bmp_dib12(declared_size: u32, pixel_offset: u32, planes: u16, bits_per_pixel: u16) -> Vec<u8> {
    let mut bmp = vec![0; 26];
    bmp[0..2].copy_from_slice(b"BM");
    write_u32_le(&mut bmp, 2, declared_size);
    write_u32_le(&mut bmp, 10, pixel_offset);
    write_u32_le(&mut bmp, 14, 12);
    write_u16_le(&mut bmp, 22, planes);
    write_u16_le(&mut bmp, 24, bits_per_pixel);
    bmp
}

fn bmp_dib40(
    declared_size: u32,
    pixel_offset: u32,
    dib_header_size: u32,
    planes: u16,
    bits_per_pixel: u16,
    length: usize,
) -> Vec<u8> {
    let mut bmp = vec![0; length];
    bmp[0..2].copy_from_slice(b"BM");
    write_u32_le(&mut bmp, 2, declared_size);
    write_u32_le(&mut bmp, 10, pixel_offset);
    write_u32_le(&mut bmp, 14, dib_header_size);
    if length >= 30 {
        write_u16_le(&mut bmp, 26, planes);
        write_u16_le(&mut bmp, 28, bits_per_pixel);
    }
    bmp
}

fn write_u16_le(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32_be(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn write_u32_le(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
