use crate::shared_image_fixtures as image_fixtures;

use pi_coding_agent::utils::photon::{PhotonError, decode_image, jpeg_bytes, png_bytes};

#[test]
fn decodes_supported_formats_to_rgba_pixels() {
    for (name, base64, width, height) in [
        ("png", image_fixtures::TINY_PNG, 2, 2),
        ("jpeg", image_fixtures::TINY_JPEG, 2, 2),
        ("large png", image_fixtures::LARGE_PNG_200X200, 200, 200),
    ] {
        let image = decode_image(&image_fixtures::decode(base64)).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(image.get_width(), width, "{name}");
        assert_eq!(image.get_height(), height, "{name}");
        assert_eq!(image.get_raw_pixels().len(), (width * height * 4) as usize, "{name}");
    }
}

#[test]
fn rejects_undecodable_bytes() {
    assert!(matches!(decode_image(b"not an image"), Err(PhotonError::Decode { .. })));
    assert!(matches!(decode_image(&[]), Err(PhotonError::Decode { .. })));
}

#[test]
fn encodes_png_and_jpeg_from_raw_pixels() {
    let image = decode_image(&image_fixtures::decode(image_fixtures::TINY_PNG)).unwrap();
    let png = png_bytes(&image).unwrap();
    assert_eq!(&png[..8], &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

    let jpeg = jpeg_bytes(&image, 80).unwrap();
    assert_eq!(&jpeg[..2], &[0xff, 0xd8]);
    assert_eq!(&jpeg[jpeg.len() - 2..], &[0xff, 0xd9]);

    assert_ne!(jpeg_bytes(&image, 40).unwrap(), jpeg);
}

#[test]
fn rejects_images_whose_pixel_buffer_does_not_match_the_dimensions() {
    let image = photon_rs::PhotonImage::new(vec![0, 0, 0], 2, 2);
    assert!(matches!(png_bytes(&image), Err(PhotonError::Encode { .. })));
}
