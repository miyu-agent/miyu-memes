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

/// 逻辑屏 65535×65535、两帧各 1×1 的 GIF，用注释块垫到 `at_least` 字节以上（审计 #130）。
#[cfg(target_os = "linux")]
fn padded_bomb(at_least: usize) -> Vec<u8> {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0x80, 0, 0]);
    gif.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
    for _ in 0..2 {
        gif.extend_from_slice(&[0x21, 0xF9, 4, 0, 10, 0, 0, 0]);
        gif.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
        gif.extend_from_slice(&[2, 2, 0x44, 0x01, 0]);
    }
    gif.extend_from_slice(&[0x21, 0xFE]);
    while gif.len() < at_least {
        gif.push(255);
        gif.extend(std::iter::repeat_n(b'x', 255));
    }
    gif.push(0);
    gif.push(0x3B);
    gif
}

#[cfg(target_os = "linux")]
#[test]
fn audit_130_a_gif_bomb_over_the_limit_is_refused_before_its_canvas_is_allocated() {
    // 审计 #130：字节超过单张上限的 GIF 先过 `animated()`，那里的 `GifDecoder::new` 不带 `Limits`，
    // 第一帧前照逻辑屏分配一整张画布：65535×65535×4 ≈ 17 GB，几百字节的文件就能把机器压到 OOM。
    // 不真分配：在子进程里把地址空间限到 2 GiB（`ulimit -v`）再调 `fit`；修好以后应在分配前就拒（Err），
    // 现在子进程会因「memory allocation of … bytes failed」中止。
    const MAX: u64 = 1024;
    if std::env::var_os("MIYU_MEMES_AUDIT_130_CHILD").is_some() {
        let bomb = padded_bomb(2048);
        assert!(bomb.len() as u64 > MAX, "要超过上限才会被解码");
        let got = fit(&bomb, MAX);
        assert!(
            matches!(got, Err(Why::TooBig | Why::NotImage)),
            "炸弹 GIF 应被拒：{got:?}"
        );
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg("ulimit -v 2097152 && exec \"$0\" audit_130_a_gif_bomb --test-threads=1 --nocapture")
        .arg(&exe)
        .env("MIYU_MEMES_AUDIT_130_CHILD", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "炸弹 GIF 不该在拒之前分配逻辑屏那么大的画布：{:?}\n{stderr}\n{stdout}",
        out.status
    );
    assert!(
        stdout.contains("1 passed"),
        "子进程要真跑到那条测试：{stdout}"
    );
}
