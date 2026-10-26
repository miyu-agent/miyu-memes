//! 五件工具被调到时怎么办（设计 34 第四节）：核心发 `tool.call`，这里照工具名办、交回结果。给她看的字写英文短句，调用以后才用得上
//! 的写在这里（作者说明第三节第 3 条），不写进工具说明。

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::library::Library;
use crate::search::{Embed, search};
use crate::settings::Settings;
use crate::wire::{Answer, Wire};

/// 核心那一头：测试里换成假的。
pub trait Core: Send + Sync {
    fn call(&self, method: &str, params: &Value) -> Answer;
}

impl Core for Wire {
    fn call(&self, method: &str, params: &Value) -> Answer {
        Wire::call(self, method, params)
    }
}

/// 经核心的 `model.embed` 算向量。
pub struct CoreEmbed<'a>(pub &'a dyn Core);

impl Embed for CoreEmbed<'_> {
    fn embed(&self, texts: &[String]) -> Option<(String, Vec<Vec<f32>>)> {
        let answer = self
            .0
            .call("model.embed", &json!({ "texts": texts }))
            .ok()?;
        let model = answer["model"].as_str()?.to_string();
        let vectors = serde_json::from_value(answer["vectors"].clone()).ok()?;
        Some((model, vectors))
    }
}

/// 几样共用的：库、配置、核心。
pub struct State {
    pub library: Mutex<Library>,
    pub settings: Mutex<Settings>,
    pub core: Arc<dyn Core>,
}

/// 这一刻，毫秒。
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

pub(crate) fn ok(text: &str) -> Value {
    json!({"blocks": [{"type": "text", "text": text}]})
}

pub(crate) fn fail(text: &str) -> Value {
    json!({"blocks": [{"type": "text", "text": text}], "error": true})
}

pub(crate) fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// 办一次 `tool.call`：`params` 是核心发来的那一份。
pub fn tool(state: &State, params: &Value) -> Value {
    let args = &params["args"];
    let library = state.library.lock().unwrap_or_else(PoisonError::into_inner);
    let done = match params["tool"].as_str().unwrap_or_default() {
        "meme_search" => find(&library, state.core.as_ref(), args),
        "meme_send" => send(&library, args),
        "meme_save" => {
            let max = state
                .settings
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .max_bytes;
            crate::save::save(&library, state.core.as_ref(), args, params, max)
        }
        "meme_edit" => edit(&library, args),
        "meme_delete" => delete(&library, args),
        _ => Ok(fail("Unknown tool.")),
    };
    done.unwrap_or_else(|error| fail(&format!("The sticker library failed: {error}")))
}

pub(crate) type Done = Result<Value, rusqlite::Error>;

fn find(library: &Library, core: &dyn Core, args: &Value) -> Done {
    let Some(query) = text(args, "query") else {
        return Ok(fail("Say what you want to express in query."));
    };
    let limit = args["limit"].as_u64().map_or(5, |n| n.clamp(1, 20));
    let found = search(
        library,
        &CoreEmbed(core),
        query,
        usize::try_from(limit).unwrap_or(5),
    )?;
    if found.is_empty() {
        return Ok(ok(
            "No sticker fits. Reply without one, or save one with meme_save.",
        ));
    }
    let lines: Vec<String> = found
        .iter()
        .map(|meme| format!("{}: {} When: {}", meme.id, meme.description, meme.usage))
        .collect();
    Ok(ok(&lines.join("\n")))
}

fn send(library: &Library, args: &Value) -> Done {
    let Some(id) = text(args, "id") else {
        return Ok(fail("Give the sticker id."));
    };
    let Some(meme) = library.get(id)?.filter(|meme| meme.enabled) else {
        return Ok(fail(&format!(
            "No sticker {id}. Find one with meme_search."
        )));
    };
    library.used(&meme.id, now())?;
    let mut done = ok(&format!("Sent {}.", meme.id));
    done["sent"] = json!([{
        "blob": meme.hash, "media_type": meme.media_type, "width": meme.width, "height": meme.height,
        "alt": meme.description, "sticker": true
    }]);
    if args["end"].as_bool().unwrap_or(true) {
        done["end_turn"] = Value::Bool(true);
    }
    Ok(done)
}

fn edit(library: &Library, args: &Value) -> Done {
    let Some(id) = text(args, "id") else {
        return Ok(fail("Give the sticker id."));
    };
    let (description, usage) = (text(args, "description"), text(args, "usage"));
    if description.is_none() && usage.is_none() {
        return Ok(fail("Give a new description or usage."));
    }
    Ok(if library.edit(id, description, usage)? {
        ok(&format!("Updated {id}."))
    } else {
        fail(&format!("No sticker {id}."))
    })
}

fn delete(library: &Library, args: &Value) -> Done {
    let Some(id) = text(args, "id") else {
        return Ok(fail("Give the sticker id."));
    };
    Ok(if library.set_enabled(id, false)? {
        ok(&format!("Disabled {id}."))
    } else {
        fail(&format!("No sticker {id}."))
    })
}

#[cfg(test)]
mod tests;
