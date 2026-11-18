//! 单张超过上限的先缩再存（项目主人 10-16 定：超了缩，不拒）：解开，照原样重编一次；还大就把长边每次缩到四分之三再编，
//! 直到装得下。有透明的编成 PNG，没有的编成 JPEG（质量 85）。动图缩了就不动了，照旧拒；长边缩到 64 像素还装不下的也拒。

use std::io::Cursor;

use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};

/// 长边最少缩到多少像素：再小就不像那张图了。
const SMALLEST: u32 = 64;

/// 解图时最多占多少内存：防一张小文件解出几 GB 的图。
pub const DECODE_BYTES: u64 = 256 * 1024 * 1024;

/// 缩不进来的原因：交给调的一方照它说的话回。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// 解不开：不是图，或者坏了。
    NotImage,
    /// 动图，或者缩到最小还是装不下。
    TooBig,
}

/// 缩好的一张：字节和该用的扩展名。
#[derive(Debug)]
pub struct Shrunk {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
}

/// `bytes` 装得进 `max` 的交回 `None`（原样存）；装不进的缩到装得进，交回缩好的。
pub fn fit(bytes: &[u8], max: u64) -> Result<Option<Shrunk>, Why> {
    if bytes.len() as u64 <= max {
        return Ok(None);
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Why::NotImage)?;
    if reader.format() == Some(ImageFormat::Gif) && animated(bytes) {
        return Err(Why::TooBig);
    }
    reader.limits(limits());
    let image = reader.decode().map_err(|_| Why::NotImage)?;
    let mut side = image.width().max(image.height());
    loop {
        let scaled = if side == image.width().max(image.height()) {
            image.clone()
        } else {
            image.resize(side, side, FilterType::Lanczos3)
        };
        let shrunk = encode(&scaled).ok_or(Why::NotImage)?;
        if shrunk.bytes.len() as u64 <= max {
            return Ok(Some(shrunk));
        }
        if side <= SMALLEST {
            return Err(Why::TooBig);
        }
        side = (side * 3 / 4).max(SMALLEST);
    }
}

/// 解图的上限：至多占 [`DECODE_BYTES`]。
pub fn limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_alloc = Some(DECODE_BYTES);
    limits
}

/// GIF 有不止一帧（前两帧都解得开）。照 [`limits`] 解：逻辑屏大得装不下的画布不分配，算不是动图，交给后面带上限的解码去拒。
pub fn animated(bytes: &[u8]) -> bool {
    let Ok(mut decoder) = GifDecoder::new(Cursor::new(bytes)) else {
        return false;
    };
    if decoder.set_limits(limits()).is_err() {
        return false;
    }
    let mut frames = decoder.into_frames();
    matches!((frames.next(), frames.next()), (Some(Ok(_)), Some(Ok(_))))
}

/// 有透明的编成 PNG，没有的编成 JPEG。
fn encode(image: &DynamicImage) -> Option<Shrunk> {
    let mut bytes = Vec::new();
    if image.color().has_alpha() {
        image
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .ok()?;
        Some(Shrunk { bytes, ext: "png" })
    } else {
        JpegEncoder::new_with_quality(&mut bytes, 85)
            .encode_image(&image.to_rgb8())
            .ok()?;
        Some(Shrunk { bytes, ext: "jpg" })
    }
}

/// 三处存图（传图、她存、内容包）都照这个：装得下的原样交回，装不下的交回缩好的字节和换了扩展名的名字。
pub fn fitted(bytes: Vec<u8>, name: &str, max: u64) -> Result<(Vec<u8>, String), Why> {
    Ok(match fit(&bytes, max)? {
        None => (bytes, name.to_string()),
        Some(shrunk) => (shrunk.bytes, renamed(name, shrunk.ext)),
    })
}

/// 文件名换成 `ext` 的扩展名：`a.png` → `a.jpg`，没有扩展名的接上。
pub fn renamed(name: &str, ext: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{stem}.{ext}")
}

#[cfg(test)]
mod tests;
