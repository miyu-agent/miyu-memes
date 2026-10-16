//! 让看图模型写描述和用法（项目主人 10-16：后台页「重新描述」、传图时「让 AI 写」）：经核心的 `model.call`，`use: "vision"`
//! 照人配的看图模型（核心施工 X-23）。写好的交回页面填进框里，人看了再存，这里不存。给模型的字登记在 `docs/prompt-ledger.md`。

use serde_json::{Value, json};

use crate::calls::Core;

/// 给看图模型的 system（登记在 `docs/prompt-ledger.md`「写描述」那一行）。
const SYSTEM: &str = "You label a sticker image for a sticker library. Reply with exactly two lines.\n\
Description: what the image shows, including any words written in it.\n\
Usage: when it fits as a reply in a chat.\n\
Write both lines in the language named in the request.";

/// 交回的字写得不对时的原话。
const UNREADABLE: &str = "the vision reply has no Description and Usage lines";

/// 看 `blob` 那张图写描述、用法，照 `language`（`zh`、`en`、`ja`）。交回 `(描述, 用法)`；核心拒了的、写得不对的交回错。
pub fn describe(core: &dyn Core, blob: &str, language: &str) -> Result<(String, String), Value> {
    let asked = json!({
        "use": "vision",
        "purpose": "meme-describe",
        "max_tokens": 400,
        "messages": [
            {"role": "system", "text": SYSTEM},
            {"role": "user", "text": format!("Language: {language}"), "images": [blob]},
        ],
    });
    let answer = core.call("model.call", &asked)?;
    parse(answer["text"].as_str().unwrap_or_default())
        .ok_or_else(|| json!({"code": -32000, "message": UNREADABLE}))
}

/// 从回答里取 `Description:`、`Usage:` 两行（不分大小写，冒号中英文都认）；有一行没有的不算。
fn parse(text: &str) -> Option<(String, String)> {
    let mut description = None;
    let mut usage = None;
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['*', '-', ' ']);
        let Some((head, rest)) = line.split_once([':', '：']) else {
            continue;
        };
        let rest = rest.trim().trim_matches('*').trim();
        if rest.is_empty() {
            continue;
        }
        match head.trim().trim_matches('*').to_ascii_lowercase().as_str() {
            "description" => description = Some(rest.to_string()),
            "usage" => usage = Some(rest.to_string()),
            _ => {}
        }
    }
    Some((description?, usage?))
}

#[cfg(test)]
mod tests;
