//! 后台页列表用的小图（施工 M-14）：列表一屏几十张，张张取原图又慢，大的还撑破核心收的一行 1 MiB。这里把一张原图做成长边
//! [`SIDE`] 以内的小图，缓存在包的状态目录里。
//!
//! - 怎么做（[`make`]）：不透明的编成 JPEG（质量 80，还大就 60），有透明的编成无损 WebP（`image` 的 WebP 只能编无损；
//!   比 PNG 小）；超过 [`MOST`] 的把长边每次缩到四分之三再编，到 64 像素还装不下就拒。动图取第一帧。
//!   本来就不大、不是动图、字节也在 [`MOST`] 以内的原样交回，一个字节不动。
//! - 缓存（[`cached`]、[`keep`]、[`forget`]）：库旁边的 `thumbs/`，文件名是图的哈希（`sha256:` 后面那 64 位十六进制），没有扩展名，
//!   类型读的时候照内容认。认不出来、读不了的当没有，调的一方重做。先写临时文件再改名，读的一方见不到写了一半的。

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, Limits};

use crate::shrink::{DECODE_BYTES, Why, animated};

/// 小图的长边最多多少像素。
pub const SIDE: u32 = 256;

/// 小图最多多少字节：写成 base64 是 256 KiB，离核心收的一行 1 MiB 远。
pub const MOST: usize = 192 * 1024;

/// 长边最少缩到多少像素。
const SMALLEST: u32 = 64;

/// 一张小图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
}

/// 把原图 `bytes` 做成小图（模块的说明）。
pub fn make(bytes: Vec<u8>) -> Result<Thumb, Why> {
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| Why::NotImage)?;
    let format = reader.format().ok_or(Why::NotImage)?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(DECODE_BYTES);
    reader.limits(limits);
    // 动图解出来的是第一帧。
    let image = reader.decode().map_err(|_| Why::NotImage)?;
    let longest = image.width().max(image.height());
    if longest <= SIDE && bytes.len() <= MOST && !moving(format, &bytes) {
        return Ok(Thumb {
            media_type: format.to_mime_type(),
            bytes,
        });
    }
    let mut side = longest.min(SIDE);
    loop {
        let scaled = if side < longest {
            image.resize(side, side, FilterType::Triangle)
        } else {
            image.clone()
        };
        for thumb in encoded(&scaled) {
            if thumb.bytes.len() <= MOST {
                return Ok(thumb);
            }
        }
        if side <= SMALLEST {
            return Err(Why::TooBig);
        }
        side = (side * 3 / 4).max(SMALLEST);
    }
}

/// 不止一帧：GIF、WebP、APNG。
fn moving(format: ImageFormat, bytes: &[u8]) -> bool {
    match format {
        ImageFormat::Gif => animated(bytes),
        ImageFormat::WebP => {
            WebPDecoder::new(Cursor::new(bytes)).is_ok_and(|decoder| decoder.has_animation())
        }
        ImageFormat::Png => PngDecoder::new(Cursor::new(bytes))
            .is_ok_and(|decoder| decoder.is_apng().unwrap_or(false)),
        _ => false,
    }
}

/// 编出来的几种，照先后试：不透明的 JPEG 质量 80、60；有透明的无损 WebP。
fn encoded(image: &DynamicImage) -> Vec<Thumb> {
    let rgba = image.to_rgba8();
    if rgba.pixels().any(|pixel| pixel[3] != u8::MAX) {
        let mut bytes = Vec::new();
        let done = DynamicImage::ImageRgba8(rgba)
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::WebP)
            .is_ok();
        let media_type = ImageFormat::WebP.to_mime_type();
        return Vec::from_iter(done.then_some(Thumb { media_type, bytes }));
    }
    let rgb = image.to_rgb8();
    [80, 60]
        .into_iter()
        .filter_map(|quality| {
            let mut bytes = Vec::new();
            JpegEncoder::new_with_quality(&mut bytes, quality)
                .encode_image(&rgb)
                .ok()?;
            let media_type = ImageFormat::Jpeg.to_mime_type();
            Some(Thumb { media_type, bytes })
        })
        .collect()
}

/// 缓存里 `hash` 那一张放哪：`sha256:` 后面是 64 位十六进制的才有（文件名只用这一段，带不进别的路径）。
fn place(dir: &Path, hash: &str) -> Option<PathBuf> {
    let hex = hash.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| dir.join(hex))
}

/// 缓存里的那一张：没有、读不了、不是图、大过 [`MOST`] 的都是 `None`。
pub fn cached(dir: &Path, hash: &str) -> Option<Thumb> {
    let bytes = std::fs::read(place(dir, hash)?).ok()?;
    if bytes.len() > MOST {
        return None;
    }
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    let media_type = reader.format()?.to_mime_type();
    reader.into_dimensions().ok()?;
    Some(Thumb { media_type, bytes })
}

/// 临时文件名里的序号：同一个进程里几个线程同时做同一张也不撞。
static NEXT: AtomicU64 = AtomicU64::new(0);

/// 把做好的小图存进缓存。存不了的不算错：下次再做。
pub fn keep(dir: &Path, hash: &str, bytes: &[u8]) {
    let Some(file) = place(dir, hash) else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let partial = dir.join(format!(
        ".{}-{}.part",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::write(&partial, bytes).is_err() || std::fs::rename(&partial, &file).is_err() {
        drop(std::fs::remove_file(&partial));
    }
}

/// 这张图不在库里了：它的小图拿掉。
pub fn forget(dir: &Path, hash: &str) {
    if let Some(file) = place(dir, hash) {
        drop(std::fs::remove_file(file));
    }
}

#[cfg(test)]
pub(crate) mod tests;
