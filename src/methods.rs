//! 后台页和别的软件调的方法（设计 34 第七节、第九节）：核心发 `method.call {method, params, from?}`，`from` 是调的那个包（核心填，
//! 核心施工 X-18）；人的后台页调的没有 `from`。图都先经 `blob.put` 传给核心，这里只收 blob 和它的宽高。

use serde_json::{Value, json};

mod images;

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::calls::{CoreEmbed, State, now};
use crate::describe::describe;
use crate::library::{Library, NewMeme};
use crate::search::reindex;
use crate::search::search;
use crate::wire::Answer;

/// 登记的方法：后台页的十个，加给别的软件的两个（清单 `[export]`）。
pub const NAMES: [&str; 12] = [
    "list", "get", "image", "update", "delete", "restore", "purge", "upload", "add", "search",
    "describe", "reindex",
];

/// `package.methods` 的参数。
pub fn register() -> Value {
    let methods: Vec<Value> = NAMES.iter().map(|name| json!({ "name": name })).collect();
    json!({ "methods": methods })
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 要写成哪种语言：页面交的 `language`（2 到 8 个小写字母），不合的照英文。
fn language(params: &Value) -> &str {
    params["language"]
        .as_str()
        .filter(|l| (2..=8).contains(&l.len()) && l.bytes().all(|b| b.is_ascii_lowercase()))
        .unwrap_or("en")
}

fn bad(why: &str) -> Value {
    json!({"code": -32602, "message": why})
}

fn id_of(params: &Value) -> Result<&str, Value> {
    params["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| bad("id is required"))
}

/// 办一次 `method.call`。
pub fn call(state: &State, request: &Value) -> Answer {
    let params = &request["params"];
    let from = request["from"].as_str();
    // 要等核心的两样先办，不拿着库的锁等。
    match request["method"].as_str().unwrap_or_default() {
        "image" => {
            let id = id_of(params)?;
            let meme = locked(&state.library)
                .get(id)
                .map_err(|error| json!({"code": -32000, "message": error.to_string()}))?
                .ok_or_else(|| json!({"code": -32000, "message": "no such sticker"}))?;
            return images::image(state.core.as_ref(), &meme);
        }
        "upload" => {
            let max = locked(&state.settings).max_bytes;
            let stored = images::store(state.core.as_ref(), params, max)?;
            return put(&locked(&state.library), &stored, None);
        }
        // 让看图模型写描述、用法（项目主人 10-16）：库里的那一张写 `id`，还没传的写 `data`、`name`（先存进核心）。不存。
        "describe" => {
            let blob = if let Some(id) = params["id"].as_str() {
                locked(&state.library)
                    .get(id)
                    .map_err(|error| json!({"code": -32000, "message": error.to_string()}))?
                    .ok_or_else(|| json!({"code": -32000, "message": "no such sticker"}))?
                    .hash
            } else {
                let max = locked(&state.settings).max_bytes;
                let stored = images::store(state.core.as_ref(), params, max)?;
                stored["blob"].as_str().unwrap_or_default().to_string()
            };
            let (description, usage) = describe(state.core.as_ref(), &blob, language(params))?;
            return Ok(json!({ "description": description, "usage": usage }));
        }
        _ => {}
    }
    let library = locked(&state.library);
    let done = match request["method"].as_str().unwrap_or_default() {
        "list" => library.all(false).map(|memes| json!({ "memes": memes })),
        "get" => {
            let id = id_of(params)?;
            library.get(id).map(|meme| json!({ "meme": meme }))
        }
        "update" => {
            let id = id_of(params)?;
            library
                .edit(id, params["description"].as_str(), params["usage"].as_str())
                .map(|found| json!({ "found": found }))
        }
        "delete" | "restore" => {
            let id = id_of(params)?;
            let enabled = request["method"] == "restore";
            library
                .set_enabled(id, enabled)
                .map(|found| json!({ "found": found }))
        }
        "purge" => {
            let id = id_of(params)?;
            library.purge(id).map(|found| json!({ "found": found }))
        }
        "add" => return put(&library, params, from),
        "search" => {
            let query = params["query"].as_str().unwrap_or_default();
            let limit = params["limit"].as_u64().map_or(5, |n| n.clamp(1, 50));
            search(
                &library,
                &CoreEmbed(state.core.as_ref()),
                query,
                usize::try_from(limit).unwrap_or(5),
            )
            .map(|memes| json!({ "memes": memes }))
        }
        // 重新生成语义索引：照现在的 embedding 模型重算每一张；没开 embedding 的 `embedding_off`。
        "reindex" => match reindex(&library, &CoreEmbed(state.core.as_ref())) {
            Ok(Some((model, count))) => Ok(json!({ "model": model, "count": count })),
            Ok(None) => return Err(json!({"code": -32000, "message": "embedding_off"})),
            Err(error) => Err(error),
        },
        _ => return Err(crate::wire::unknown_method()),
    };
    done.map_err(|error| json!({"code": -32000, "message": error.to_string()}))
}

/// 放一张进库：`blob`、`media_type`、`width`、`height`、`description`、`usage` 必写，`reason`、`origin`（别的软件交的「从哪来」，
/// 原样存）可以不写。人传的记 `human`，别的软件放的记 `package` 和它的编号。
fn put(library: &Library, params: &Value, from: Option<&str>) -> Answer {
    let text = |key: &str| {
        params[key]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let (Some(blob), Some(media_type), Some(description), Some(usage)) = (
        text("blob"),
        text("media_type"),
        text("description"),
        text("usage"),
    ) else {
        return Err(bad("blob, media_type, description and usage are required"));
    };
    let size = |key: &str| {
        params[key]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n > 0)
    };
    let (Some(width), Some(height)) = (size("width"), size("height")) else {
        return Err(bad("width and height are required"));
    };
    let mut source = match from {
        Some(package) => json!({"kind": "package", "package": package}),
        None => json!({"kind": "human"}),
    };
    if let (Some(origin), Some(object)) = (params["origin"].as_object(), source.as_object_mut()) {
        for (key, value) in origin {
            object.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    let new = NewMeme {
        hash: blob.to_string(),
        media_type: media_type.to_string(),
        width,
        height,
        description: description.to_string(),
        usage: usage.to_string(),
        source,
        reason: text("reason").map(str::to_string),
    };
    library
        .add(&new, now())
        .map(|(id, created)| json!({ "id": id, "created": created }))
        .map_err(|error| json!({"code": -32000, "message": error.to_string()}))
}

#[cfg(test)]
mod tests;
