//! 把字节交给核心存成 blob（施工 M-12）：核心收一行最多 1 MiB，超了直接断开连接（`endpoint line too long, closed`）。原来不管多大
//! 都整张写成 base64 塞进一条 `blob.put`，大于约 750 KB 的图一发就断，表情包程序读到断开就退出：2026-11-05 主人的部署装了旧版
//! 迁过来的内容包（有 10 MB 的 gif），表情包程序起来一同步就断，连着五次被核心停掉，一整天没在跑。
//!
//! - [`PUT_MOST`] 以内的照旧一条 `blob.put`（一行放得下，少几次来回）；
//! - 大的分块传：`blob.open {name, size}`、一块一块 `blob.write {upload, offset, data}`（每块 [`CHUNK`] 字节写成 base64）、
//!   `blob.close {upload}`。回应和 `blob.put` 一样（`blob`、`kind`、`media_type`、`width`、`height`）。核心照 `received` 说收到
//!   哪了，往前走不动的当失败交回，不死循环。

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use crate::calls::Core;
use crate::wire::Answer;

/// 一条 `blob.put` 最多交多少字节：写成 base64 约 683 KiB，加上别的格也在一行 1 MiB 以内。
pub const PUT_MOST: usize = 512 * 1024;

/// 分块传时一块多少字节（核心收的一块最多 512 KiB）。
pub const CHUNK: usize = 256 * 1024;

/// 把 `bytes`（文件名 `name`）交给核心存成 blob，交回核心的回应（模块的说明）。
pub fn upload(core: &dyn Core, name: &str, bytes: &[u8]) -> Answer {
    if bytes.len() <= PUT_MOST {
        return core.call(
            "blob.put",
            &json!({"data": STANDARD.encode(bytes), "name": name}),
        );
    }
    let opened = core.call("blob.open", &json!({"name": name, "size": bytes.len()}))?;
    let upload = opened["upload"].clone();
    let mut offset = 0;
    while offset < bytes.len() {
        let end = (offset + CHUNK).min(bytes.len());
        let data = STANDARD.encode(&bytes[offset..end]);
        let got = core.call(
            "blob.write",
            &json!({"upload": upload, "offset": offset, "data": data}),
        )?;
        let received = got["received"]
            .as_u64()
            .and_then(|at| usize::try_from(at).ok())
            .unwrap_or(end);
        if received <= offset {
            return Err(stuck(offset));
        }
        offset = received;
    }
    core.call("blob.close", &json!({"upload": upload}))
}

/// 核心收不往前走了：交回一句，同核心拒的样子。
fn stuck(offset: usize) -> Value {
    json!({"message": format!("upload stuck at {offset}")})
}

#[cfg(test)]
mod tests;
