//! 她存一张（`meme_save`，设计 34 第四节；施工 M-9，项目主人 10-26）：图是本机文件（`path`），或者聊天里一条消息的第几张（`msg`、
//! `index`，经 im 的 `chat.media` 取，带来历：哪个场所、哪一条、谁发的、什么时候）。她只说为什么存（`reason`，必写）；描述、用法
//! 交模型写：先照人配的看图模型，它做不成的照主模型（`describe::label`），都写不成的不存。同一张已经在库里的直接交回编号，
//! 不再请模型。谁叫的那一轮都能存（项目主人 10-26）。

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use crate::calls::{Core, Done, fail, now, ok, text};
use crate::describe::label;
use crate::library::{Library, NewMeme};
use crate::shrink::{Why, fitted};

/// 一张要存的图：交给核心 `blob.put` 的参数，聊天里来的带来历。
struct Picture {
    put: Value,
    origin: Option<Value>,
}

/// 存一张：`args` 是她给的参数，`call` 是核心的 `tool.call`（带 `session`、`cwd`），`max` 是单张最多多少字节（超了先缩）。
pub fn save(library: &Library, core: &dyn Core, args: &Value, call: &Value, max: u64) -> Done {
    let Some(reason) = text(args, "reason") else {
        return Ok(fail("Say why you keep it in reason."));
    };
    let picture = match (text(args, "path"), Wanted::read(args)) {
        (_, Some(Ok(wanted))) => from_chat(core, call["session"].as_str(), &wanted, max),
        (_, Some(Err(()))) => Err(fail("index counts from 1.")),
        (Some(path), None) => from_file(path, call["cwd"].as_str(), max),
        (None, None) => Err(fail("Give path, or msg and index.")),
    };
    let picture = match picture {
        Ok(picture) => picture,
        Err(said) => return Ok(said),
    };
    let Ok(put) = core.call("blob.put", &picture.put) else {
        return Ok(fail("Cannot read that image."));
    };
    if put["kind"] != "image" {
        return Ok(fail("That file is not an image."));
    }
    let hash = put["blob"].as_str().unwrap_or_default();
    if let Some(id) = library.id_of(hash)? {
        return Ok(ok(&format!("Already saved as {id}.")));
    }
    let (description, usage) = match label(core, hash, language(reason)) {
        Ok(labelled) => labelled,
        Err(error) => {
            let why = error["message"].as_str().unwrap_or("the model failed");
            return Ok(fail(&format!(
                "Not saved, it could not be described: {why}"
            )));
        }
    };
    let mut source = json!({"kind": "ai"});
    if let (Some(Value::Object(origin)), Some(object)) = (picture.origin, source.as_object_mut()) {
        object.extend(origin);
    }
    let new = NewMeme {
        hash: hash.to_string(),
        media_type: put["media_type"].as_str().unwrap_or_default().to_string(),
        width: u32::try_from(put["width"].as_u64().unwrap_or(0)).unwrap_or(0),
        height: u32::try_from(put["height"].as_u64().unwrap_or(0)).unwrap_or(0),
        description,
        usage,
        source,
        reason: Some(reason.to_string()),
    };
    let (id, _) = library.add(&new, now())?;
    Ok(ok(&format!("Saved {id}.")))
}

/// 聊天里的哪一张：消息的平台编号（她看到的 `[msg=…]`）、第几张（`#n`，从 1 数）。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Wanted {
    msg: String,
    index: u64,
}

impl Wanted {
    /// 读参数：没写 `msg` 的是空的；`msg` 是字或整数，`index` 不写是 1，写了不是正整数的交回错。
    fn read(args: &Value) -> Option<Result<Wanted, ()>> {
        let msg = match &args["msg"] {
            Value::String(msg) if !msg.trim().is_empty() => msg.trim().to_string(),
            Value::Number(msg) if msg.is_i64() => msg.to_string(),
            _ => return None,
        };
        let index = match &args["index"] {
            Value::Null => Some(1),
            index => index.as_u64().filter(|index| *index > 0),
        };
        Some(index.map(|index| Wanted { msg, index }).ok_or(()))
    }
}

/// 照 im 的 `chat.media` 取聊天里的那一张（会话 `session` 所在的场所），超了 `max` 的先缩。没取到的交回给她的那一句。
fn from_chat(
    core: &dyn Core,
    session: Option<&str>,
    wanted: &Wanted,
    max: u64,
) -> Result<Picture, Value> {
    let Some(session) = session else {
        return Err(fail("msg works only in a chat."));
    };
    let asked = json!({"package": "im", "method": "chat.media",
        "params": {"session": session, "msg": wanted.msg, "index": wanted.index}});
    let answer = core.call("package.call", &asked).map_err(|error| {
        let (msg, index) = (&wanted.msg, wanted.index);
        // im 拒的原因：核心的 `package.call` 转回来放在 `data.message`（被调的包写的 `message`）。
        fail(
            &match error["data"]["message"].as_str().unwrap_or_default() {
                "not_found" => format!("No message {msg} in this chat."),
                "no_item" => format!("Message {msg} has no item #{index}."),
                "not_image" => format!("Item #{index} of message {msg} is not an image."),
                _ => "Cannot get that image from the chat.".to_string(),
            },
        )
    })?;
    let Some(bytes) = answer["data"]
        .as_str()
        .and_then(|data| STANDARD.decode(data).ok())
    else {
        return Err(fail("Cannot get that image from the chat."));
    };
    let name = answer["name"].as_str().map_or_else(
        || format!("msg-{}-{}", wanted.msg, wanted.index),
        str::to_string,
    );
    let (bytes, name) = fitted(bytes, &name, max).map_err(|why| too_big(why, max))?;
    Ok(Picture {
        put: json!({"data": STANDARD.encode(bytes), "name": name}),
        origin: Some(answer["origin"].clone()),
    })
}

/// 本机文件 `path`（相对的照这一轮的工作目录 `cwd`），超了 `max` 的先读进来缩好按字节交。
fn from_file(path: &str, cwd: Option<&str>, max: u64) -> Result<Picture, Value> {
    let path = resolve(path, cwd);
    let put = match std::fs::metadata(&path) {
        // 超了先缩（`shrink.rs`）：读进来缩好，按字节交给核心。
        Ok(meta) if meta.len() > max => {
            let Ok(bytes) = std::fs::read(&path) else {
                return Err(fail("Cannot read that file."));
            };
            let name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let (bytes, name) = fitted(bytes, &name, max).map_err(|why| too_big(why, max))?;
            json!({ "data": STANDARD.encode(bytes), "name": name })
        }
        Ok(_) => json!({ "path": path }),
        Err(_) => return Err(fail("Cannot read that file.")),
    };
    Ok(Picture { put, origin: None })
}

/// 缩不成的那一句。
fn too_big(why: Why, max: u64) -> Value {
    match why {
        Why::NotImage => fail("That file is not an image."),
        Why::TooBig => fail(&format!(
            "The image is over {} MiB and cannot be shrunk to fit: it is animated or too large.",
            max / 1024 / 1024
        )),
    }
}

/// 相对路径照这一轮的工作目录。
fn resolve(path: &str, cwd: Option<&str>) -> PathBuf {
    let path = Path::new(path);
    match cwd {
        Some(cwd) if path.is_relative() => Path::new(cwd).join(path),
        _ => path.to_path_buf(),
    }
}

/// 描述照她写的理由的语言写（核心的 `tool.call` 不带语言；旧库是中文的，她用中文说的存进来也是中文）：有假名的是 `ja`，有汉字
/// 的是 `zh`，别的 `en`。
fn language(reason: &str) -> &'static str {
    if reason.chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}')) {
        "ja"
    } else if reason.chars().any(|c| matches!(c, '\u{4e00}'..='\u{9fff}')) {
        "zh"
    } else {
        "en"
    }
}

#[cfg(test)]
mod tests;
