//! 后台页的图进出：框里的页面只能经网页的 `call` 调到这里，拿不到核心的 blob，也调不了 `blob.put`（`package-pages.md`「框和
//! 网页之间怎么说」）。看图经这里分块 `blob.get` 拼回 base64；传图把页面读好的 base64 交给 `blob.put {data, name}`。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use crate::calls::Core;
use crate::library::Meme;
use crate::shrink::{Why, fitted};
use crate::wire::Answer;

/// 一次读多少：协议上一块最多 512 KiB；取 3 的倍数，中间每块的 base64 没有补位，接起来就是整张的。
const CHUNK: u64 = 174_762 * 3;

fn failed(why: &str) -> Value {
    json!({"code": -32000, "message": why})
}

/// `image {id}` → `{media_type, data}`：整张图的 base64。库的锁不拿着：调用的一方先查出这一张。
pub fn image(core: &dyn Core, meme: &Meme) -> Answer {
    let mut data = String::new();
    let mut offset = 0;
    loop {
        let got = core.call(
            "blob.get",
            &json!({"blob": meme.hash, "offset": offset, "length": CHUNK}),
        )?;
        let chunk = got["data"].as_str().unwrap_or_default();
        let size = got["size"].as_u64().unwrap_or(0);
        offset += CHUNK;
        if offset < size && chunk.ends_with('=') {
            return Err(failed("the core returned a short chunk"));
        }
        data.push_str(chunk);
        if offset >= size || chunk.is_empty() {
            break;
        }
    }
    Ok(json!({"media_type": meme.media_type, "data": data}))
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
