//! 后台页和别的软件调的方法（设计 34 第七节、第九节）：核心发 `method.call {method, params, from?}`，`from` 是调的那个包（核心填，
//! 核心施工 X-18）；人的后台页调的没有 `from`。图都先经 `blob.put` 传给核心，这里只收 blob 和它的宽高。

use serde_json::{Value, json};

use crate::calls::{CoreEmbed, State, now};
use crate::library::{Library, NewMeme};
use crate::search::search;
use crate::wire::Answer;

/// 登记的方法：后台页的七个，加给别的软件的两个（清单 `[export]`）。
pub const NAMES: [&str; 9] = [
    "list", "get", "update", "delete", "restore", "purge", "upload", "add", "search",
];

/// `package.methods` 的参数。
pub fn register() -> Value {
    let methods: Vec<Value> = NAMES.iter().map(|name| json!({ "name": name })).collect();
    json!({ "methods": methods })
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
    let library = state
        .library
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        "upload" | "add" => return put(&library, params, from),
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
