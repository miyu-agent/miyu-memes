use std::io::Cursor;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, DynamicImage, Frame, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

use super::{Why, fit, renamed};

/// 一张噪点图：编出来大，缩了才装得下。
fn noisy(width: u32, height: u32, alpha: bool) -> Vec<u8> {
    let mut seed = 0x2545_F491_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        u8::try_from(seed & 0xFF).unwrap_or(0)
    };
    let image = if alpha {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(width, height, |_, _| {
            Rgba([next(), next(), next(), next()])
        }))
    } else {
        DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |_, _| {
            Rgb([next(), next(), next()])
        }))
    };
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

fn decoded(bytes: &[u8]) -> DynamicImage {
    image::load_from_memory(bytes).unwrap()
}

#[test]
fn a_picture_that_fits_is_kept_as_it_is() {
    let bytes = noisy(32, 32, false);
    assert!(fit(&bytes, bytes.len() as u64).unwrap().is_none());
}

#[test]
fn a_big_picture_is_shrunk_until_it_fits() {
    let bytes = noisy(800, 600, false);
    let max = 60 * 1024;
    assert!(bytes.len() as u64 > max, "测试图本身要超过上限");
    let shrunk = fit(&bytes, max).unwrap().unwrap();
    assert!(shrunk.bytes.len() as u64 <= max);
    assert_eq!(shrunk.ext, "jpg", "没有透明的编成 JPEG");
    let image = decoded(&shrunk.bytes);
    assert_eq!(
        image.width() * 600,
        image.height() * 800,
        "长宽比不变（缩的时候照比例）"
    );
}

#[test]
fn a_picture_with_transparency_stays_png() {
    let bytes = noisy(400, 400, true);
    let max = bytes.len() as u64 / 3;
    let shrunk = fit(&bytes, max).unwrap().unwrap();
    assert_eq!(shrunk.ext, "png");
    assert!(decoded(&shrunk.bytes).color().has_alpha());
    assert!(shrunk.bytes.len() as u64 <= max);
}

#[test]
fn an_animation_or_a_hopeless_size_is_refused() {
    let mut gif = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut gif);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for shade in [0_u8, 255] {
            let frame = RgbaImage::from_pixel(64, 64, Rgba([shade, shade, shade, 255]));
            encoder
                .encode_frame(Frame::from_parts(
                    frame,
                    0,
                    0,
                    Delay::from_numer_denom_ms(100, 1),
                ))
                .unwrap();
        }
    }
    assert_eq!(fit(&gif, 10).unwrap_err(), Why::TooBig, "动图不缩");
    let bytes = noisy(200, 200, false);
    assert_eq!(
        fit(&bytes, 100).unwrap_err(),
        Why::TooBig,
        "缩到最小还装不下"
    );
    assert_eq!(fit(b"not an image at all", 4).unwrap_err(), Why::NotImage);
}

#[test]
fn the_extension_follows_the_new_format() {
    assert_eq!(renamed("a.png", "jpg"), "a.jpg");
    assert_eq!(renamed("weird.name.webp", "png"), "weird.name.png");
    assert_eq!(renamed("noext", "jpg"), "noext.jpg");
}
