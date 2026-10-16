//! 读内容包（设计 34 第三节「内容包」，核心施工 X-22）：写了 `[content.miyu-memes] dir` 的包，经 `content.list` 列、
//! `content.read` 读。目录里 `index.json` 是 `{"memes": [{"file", "description", "usage", "reason"?}]}`（旧版的那一份原样认，
//! 别的格不看），`file` 是目录里的相对路径。起来时读一遍，核心推 `content.changed` 再读一遍；`index.json` 没变的包不再读图。
//! 同一时刻只读一遍。

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use crate::calls::{Core, State, now};
use crate::library::NewMeme;

/// 同一时刻只读一遍：两次推送挨着来，后一遍等前一遍读完。
static SYNCING: Mutex<()> = Mutex::new(());

/// 读完一遍的账：读了几个包、新放进几张、拿掉几张、跳过几张（读不到、太大、不是图）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Synced {
    pub packs: usize,
    pub added: usize,
    pub removed: usize,
    pub skipped: usize,
}

/// 照核心这一刻列的内容包把库对一遍。
pub fn sync(state: &State) -> Result<Synced, String> {
    let _one = SYNCING.lock().unwrap_or_else(PoisonError::into_inner);
    let core = state.core.as_ref();
    let listed = core
        .call("content.list", &json!({}))
        .map_err(|error| error.to_string())?;
    let packages: Vec<String> = listed["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|one| one["package"].as_str().map(str::to_string))
        .collect();
    let max = state
        .settings
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .max_bytes;
    let mut synced = Synced::default();
    for package in &packages {
        let Ok(index) = read_all(core, package, "index.json") else {
            continue;
        };
        let digest = digest(&index);
        if lib(state)
            .pack_digest(package)
            .map_err(|e| e.to_string())?
            .as_deref()
            == Some(&digest)
        {
            continue;
        }
        synced.packs += 1;
        let keep = import(state, package, &index, max, &mut synced)?;
        let library = lib(state);
        synced.removed += library
            .prune_pack(package, &keep)
            .map_err(|e| e.to_string())?;
        library
            .set_pack(package, &digest)
            .map_err(|e| e.to_string())?;
    }
    let gone: Vec<String> = lib(state)
        .packs()
        .map_err(|e| e.to_string())?
        .into_iter()
        .filter(|package| !packages.contains(package))
        .collect();
    for package in gone {
        synced.removed += lib(state)
            .forget_pack(&package)
            .map_err(|e| e.to_string())?;
    }
    Ok(synced)
}

fn lib(state: &State) -> std::sync::MutexGuard<'_, crate::library::Library> {
    state.library.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 照 `index.json` 一张张读进来；交回这个包现在带的图（哈希）。
fn import(
    state: &State,
    package: &str,
    index: &[u8],
    max: u64,
    synced: &mut Synced,
) -> Result<HashSet<String>, String> {
    let index: Value = serde_json::from_slice(index).map_err(|error| error.to_string())?;
    let mut keep = HashSet::new();
    for entry in index["memes"].as_array().into_iter().flatten() {
        let text = |key: &str| entry[key].as_str().map(str::trim).filter(|t| !t.is_empty());
        let (Some(file), Some(description), Some(usage)) =
            (text("file"), text("description"), text("usage"))
        else {
            synced.skipped += 1;
            continue;
        };
        let Some(put) = read_all(state.core.as_ref(), package, file)
            .ok()
            .filter(|bytes| bytes.len() as u64 <= max)
            .and_then(|bytes| put(state.core.as_ref(), file, &bytes))
        else {
            synced.skipped += 1;
            continue;
        };
        let hash = put["blob"].as_str().unwrap_or_default().to_string();
        let new = NewMeme {
            hash: hash.clone(),
            media_type: put["media_type"].as_str().unwrap_or_default().to_string(),
            width: size(&put["width"]),
            height: size(&put["height"]),
            description: description.to_string(),
            usage: usage.to_string(),
            source: json!({"kind": "pack", "package": package}),
            reason: text("reason").map(str::to_string),
        };
        let (_, fresh) = lib(state).add(&new, now()).map_err(|e| e.to_string())?;
        synced.added += usize::from(fresh);
        keep.insert(hash);
    }
    Ok(keep)
}

fn size(value: &Value) -> u32 {
    u32::try_from(value.as_u64().unwrap_or(0)).unwrap_or(0)
}

/// 交给核心存成 blob，是图的交回 `blob.put` 的回应。
fn put(core: &dyn Core, file: &str, bytes: &[u8]) -> Option<Value> {
    let name = file.rsplit('/').next().unwrap_or(file);
    let put = core
        .call(
            "blob.put",
            &json!({"data": STANDARD.encode(bytes), "name": name}),
        )
        .ok()?;
    (put["kind"] == "image").then_some(put)
}

/// 分块读一份，拼成整份。
fn read_all(core: &dyn Core, package: &str, path: &str) -> Result<Vec<u8>, Value> {
    let mut bytes = Vec::new();
    loop {
        let got = core.call(
            "content.read",
            &json!({"package": package, "path": path, "offset": bytes.len()}),
        )?;
        let chunk = STANDARD
            .decode(got["data"].as_str().unwrap_or_default())
            .map_err(|error| json!({"message": error.to_string()}))?;
        let eof = got["eof"].as_bool().unwrap_or(true);
        if chunk.is_empty() && !eof {
            return Err(json!({"message": "empty chunk before the end"}));
        }
        bytes.extend_from_slice(&chunk);
        if eof {
            return Ok(bytes);
        }
    }
}

/// `index.json` 的摘要（FNV-1a 64 位）：只用来比变没变。
fn digest(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests;
