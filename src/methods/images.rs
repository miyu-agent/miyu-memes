//! 后台页的图进出：框里的页面只能经网页的 `call` 调到这里，拿不到核心的 blob，也调不了 `blob.put`（`package-pages.md`「框和
//! 网页之间怎么说」）。看图经这里从核心 `blob.get`：列表要小图（`thumb`），详情要原图、一块一块给（`image`），哪一条回应都
//! 远小于核心收的一行 1 MiB（施工 M-14）；传图把页面读好的 base64 交给 `blob.put {data, name}`。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use std::path::Path;

use crate::calls::Core;
use crate::library::Meme;
use crate::shrink::{Why, fitted};
use crate::thumb;
use crate::wire::Answer;

/// 包自己读整张时一次读多少：协议上一块最多 512 KiB。
const CHUNK: u64 = 174_762 * 3;

/// `image` 一次回多少原始字节：384 KiB，写成 base64 是 512 KiB，核心收一行最多 1 MiB，余量留足。取 3 的倍数：每块的
/// base64 没有补位（最后一块除外），接起来就是整张的。
pub const PIECE: u64 = 131_072 * 3;

fn failed(why: &str) -> Value {
    json!({"code": -32000, "message": why})
}

/// `image {id, offset?}` → `{media_type, data, size, next?}`（施工 M-14）：原图从 `offset` 起的一块。`size` 是整张的字节数，
/// 后面还有的带 `next`（下一次的 `offset`）。库的锁不拿着：调用的一方先查出这一张。
pub fn image(core: &dyn Core, meme: &Meme, offset: u64) -> Answer {
    let got = core.call(
        "blob.get",
        &json!({"blob": meme.hash, "offset": offset, "length": PIECE}),
    )?;
    let data = got["data"].as_str().unwrap_or_default();
    let size = got["size"].as_u64().unwrap_or(0);
    let end = offset.saturating_add(PIECE);
    if end < size && decoded_len(data) != PIECE {
        return Err(failed("the core returned a short chunk"));
    }
    let mut answer = json!({"media_type": meme.media_type, "data": data, "size": size});
    if end < size {
        answer["next"] = json!(end);
    }
    Ok(answer)
}

/// 整张原图的字节：分块 `blob.get` 读回来。
fn whole(core: &dyn Core, hash: &str) -> Result<Vec<u8>, Value> {
    let mut bytes = Vec::new();
    loop {
        let got = core.call(
            "blob.get",
            &json!({"blob": hash, "offset": bytes.len(), "length": CHUNK}),
        )?;
        let chunk = STANDARD
            .decode(got["data"].as_str().unwrap_or_default())
            .map_err(|_| failed("the core returned a broken chunk"))?;
        let size = got["size"].as_u64().unwrap_or(0);
        let short = chunk.is_empty();
        bytes.extend(chunk);
        if bytes.len() as u64 >= size {
            return Ok(bytes);
        }
        if short {
            return Err(failed("the core returned a short chunk"));
        }
    }
}

/// `thumb {id}` → `{media_type, data}`（施工 M-14）：列表用的小图，长边 256 以内。缓存（`dir`，没有的不缓存）里有的直接给；
/// 没有、坏了的读原图现做、存进缓存。
pub fn thumb(core: &dyn Core, meme: &Meme, dir: Option<&Path>) -> Answer {
    let made = if let Some(found) = dir.and_then(|dir| thumb::cached(dir, &meme.hash)) {
        found
    } else {
        let made = thumb::make(whole(core, &meme.hash)?).map_err(|why| match why {
            Why::NotImage => failed("not_image"),
            Why::TooBig => failed("thumb_too_big"),
        })?;
        if let Some(dir) = dir {
            thumb::keep(dir, &meme.hash, &made.bytes);
        }
        made
    };
    Ok(json!({"media_type": made.media_type, "data": STANDARD.encode(made.bytes)}))
}

/// base64 `data` 解出来有多少字节。
fn decoded_len(data: &str) -> u64 {
    let padding = data.bytes().rev().take_while(|byte| *byte == b'=').count();
    (data.len() as u64 / 4 * 3).saturating_sub(padding as u64)
}

/// `upload {data, name, description, usage}`：超过上限的先缩，再 `blob.put` 存进核心，是图的才交回 `put` 要的那几格（blob、类型、宽高）。
pub fn store(core: &dyn Core, params: &Value, max_bytes: u64) -> Result<Value, Value> {
    let (Some(data), Some(name)) = (params["data"].as_str(), params["name"].as_str()) else {
        return Err(json!({"code": -32602, "message": "data and name are required"}));
    };
    // 超了先缩（`shrink.rs`），缩不进来才拒。
    let (data, name) = if decoded_len(data) > max_bytes {
        let raw = STANDARD
            .decode(data)
            .map_err(|_| json!({"code": -32602, "message": "data is not base64"}))?;
        let (raw, name) = fitted(raw, name, max_bytes).map_err(|why| match why {
            Why::TooBig => {
                json!({"code": -32000, "message": "too_big", "data": {"max_bytes": max_bytes}})
            }
            Why::NotImage => json!({"code": -32000, "message": "not_image"}),
        })?;
        (STANDARD.encode(raw), name)
    } else {
        (data.to_string(), name.to_string())
    };
    let put = core.call("blob.put", &json!({"data": data, "name": name}))?;
    if put["kind"] != "image" {
        return Err(json!({"code": -32000, "message": "not_image"}));
    }
    let mut merged = params.clone();
    for key in ["blob", "media_type", "width", "height"] {
        merged[key] = put[key].clone();
    }
    Ok(merged)
}

#[cfg(test)]
mod tests;
