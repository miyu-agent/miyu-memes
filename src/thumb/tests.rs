//! 小图（施工 M-14）：大的缩到 256 以内、装得进一行；小的原样；动图取一帧；缓存坏了当没有。

use std::io::Cursor;
use std::path::PathBuf;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, DynamicImage, Frame, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

use super::{MOST, SIDE, cached, forget, keep, make};
use crate::shrink::Why;

/// 一张噪点图（PNG）：编出来大。
pub fn noisy(width: u32, height: u32, alpha: bool) -> Vec<u8> {
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

/// 两帧的动图：头一帧黑，第二帧白。
fn animation(side: u32) -> Vec<u8> {
    let mut gif = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut gif);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for shade in [0_u8, 255] {
            let frame = RgbaImage::from_pixel(side, side, Rgba([shade, shade, shade, 255]));
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
    gif
}

/// 这个测试自己的空目录。
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("miyu-memes-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn hash(letter: &str) -> String {
    format!("sha256:{}", letter.repeat(64))
}

#[test]
fn a_big_picture_becomes_a_small_jpeg_of_the_same_shape() {
    let made = make(noisy(800, 600, false)).unwrap();
    assert_eq!(made.media_type, "image/jpeg", "没有透明的编成 JPEG");
    assert!(made.bytes.len() <= MOST);
    let image = image::load_from_memory(&made.bytes).unwrap();
    assert_eq!(
        (image.width(), image.height()),
        (SIDE, 192),
        "长边 256，比例不变"
    );
}

#[test]
fn transparency_is_kept_and_a_heavy_thumb_is_shrunk_further() {
    let made = make(noisy(400, 400, true)).unwrap();
    assert_eq!(made.media_type, "image/webp", "有透明的编成无损 WebP");
    assert!(made.bytes.len() <= MOST, "噪点图 256 见方装不下，再缩一档");
    let image = image::load_from_memory(&made.bytes).unwrap();
    assert!(image.color().has_alpha());
    assert!(image.width() < SIDE && image.width() == image.height());
}

#[test]
fn a_small_still_picture_is_kept_as_it_is() {
    let bytes = noisy(32, 24, false);
    let made = make(bytes.clone()).unwrap();
    assert_eq!(made.media_type, "image/png");
    assert_eq!(made.bytes, bytes, "一个字节不动");
}

#[test]
fn an_animation_gives_its_first_frame_even_when_small() {
    for side in [64, 600] {
        let made = make(animation(side)).unwrap();
        assert_ne!(made.media_type, "image/gif", "不再是动图");
        let image = image::load_from_memory(&made.bytes).unwrap().to_rgb8();
        assert!(image.width() <= SIDE);
        assert!(image.get_pixel(0, 0)[0] < 16, "是头一帧（黑的）");
    }
}

#[test]
fn what_is_not_a_picture_is_refused() {
    assert_eq!(
        make(b"not an image at all".to_vec()).unwrap_err(),
        Why::NotImage
    );
}

#[test]
fn the_cache_keeps_reads_and_forgets_by_hash() {
    let dir = scratch("thumb-cache").join("thumbs");
    let made = make(noisy(300, 300, false)).unwrap();
    assert_eq!(cached(&dir, &hash("a")), None, "还没存");
    keep(&dir, &hash("a"), &made.bytes);
    assert_eq!(
        cached(&dir, &hash("a")),
        Some(made),
        "目录没有的建上，读回来一样"
    );
    assert_eq!(cached(&dir, &hash("b")), None);
    forget(&dir, &hash("a"));
    assert_eq!(cached(&dir, &hash("a")), None);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "不留临时文件");
    drop(std::fs::remove_dir_all(dir.parent().unwrap()));
}

#[test]
fn a_broken_cache_file_and_an_odd_hash_count_as_nothing() {
    let dir = scratch("thumb-broken").join("thumbs");
    keep(&dir, &hash("c"), &noisy(8, 8, false));
    let file = dir.join("c".repeat(64));
    assert!(file.is_file(), "文件名照哈希");
    std::fs::write(&file, b"garbage").unwrap();
    assert_eq!(cached(&dir, &hash("c")), None, "坏了当没有");
    std::fs::write(&file, b"").unwrap();
    assert_eq!(cached(&dir, &hash("c")), None);
    keep(&dir, "sha256:../../escape", b"x");
    keep(&dir, "nonsense", b"x");
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        1,
        "不像哈希的不落盘"
    );
    assert!(
        !dir.parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("escape")
            .exists()
    );
    drop(std::fs::remove_dir_all(dir.parent().unwrap()));
}
