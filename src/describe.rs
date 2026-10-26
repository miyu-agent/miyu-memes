//! 让看图模型写描述和用法（项目主人 10-16：后台页「重新描述」、传图时「让 AI 写」）：经核心的 `model.call`，`use: "vision"`
//! 照人配的看图模型（核心施工 X-23）。写好的交回页面填进框里，人看了再存，这里不存。她存一张、自己没写描述的（施工 M-9 补）也照这里
//! 写，顺便判断是不是表情包（[`label`]）。给模型的字登记在 `docs/prompt-ledger.md`。

use serde_json::{Value, json};

use crate::calls::Core;

/// 给看图模型的 system（登记在 `docs/prompt-ledger.md`「写描述」那一行）。
const SYSTEM: &str = "You label a sticker image for a sticker library. Reply with only a JSON object: \
{\"description\": what the image shows, including any words written in it, \"usage\": when it fits as a reply in a chat}. \
Write both values in the language named in the request.";

/// 她存一张时给看图模型的 system（施工 M-9 补，项目主人 10-26：写描述时顺便判断，明显不是表情包的拒收；登记在
/// `docs/prompt-ledger.md`「存一张时写描述」那一行）。
const JUDGING: &str = "You label an image someone wants to keep as a chat sticker. Reply with only a JSON object: \
{\"sticker\": false if it is a screenshot, a photo, a document or a chart rather than a reaction image, \
\"description\": what the image shows, including any words written in it, \"usage\": when it fits as a reply in a chat}. \
Write the two texts in the language named in the request.";

/// 交回的字写得不对时的原话。
const UNREADABLE: &str = "the vision reply has no description and usage";

/// 看 `blob` 那张图写描述、用法，照 `language`（`zh`、`en`、`ja`），用人配的看图模型。交回 `(描述, 用法)`；核心拒了的、写得
/// 不对的交回错。
pub fn describe(core: &dyn Core, blob: &str, language: &str) -> Result<(String, String), Value> {
    let text = ask(core, blob, language, SYSTEM)?;
    json_of(&text)
        .or_else(|| parse(&text))
        .ok_or_else(|| json!({"code": -32000, "message": UNREADABLE}))
}

/// 看图模型写好的、判过的：描述、用法、是不是表情包。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labelled {
    pub description: String,
    pub usage: String,
    /// 不是截图、照片、文档、图表：能存。看图模型没回这一格的当是。
    pub sticker: bool,
}

/// 她存一张、自己没写描述时（施工 M-9 补，项目主人 10-26）：照人配的看图模型写描述、用法，顺便判断是不是表情包。没配看图模型
/// （`no_model`）、配的不收图（`model_failed`）、写得不对的交回错，这一张存不了；不再退到主模型。
pub fn label(core: &dyn Core, blob: &str, language: &str) -> Result<Labelled, Value> {
    let text = ask(core, blob, language, JUDGING)?;
    let (description, usage) = json_of(&text)
        .or_else(|| parse(&text))
        .ok_or_else(|| json!({"code": -32000, "message": UNREADABLE}))?;
    let sticker = text
        .find('{')
        .zip(text.rfind('}'))
        .and_then(|(start, end)| serde_json::from_str::<Value>(text.get(start..=end)?).ok())
        .and_then(|value| value["sticker"].as_bool())
        .unwrap_or(true);
    Ok(Labelled {
        description,
        usage,
        sticker,
    })
}

/// 照 `system` 问看图模型 `blob` 那张图，交回回答的正文；核心拒了的交回具体的原因（[`specific`]）。
fn ask(core: &dyn Core, blob: &str, language: &str, system: &str) -> Result<String, Value> {
    let asked = json!({
        "use": "vision",
        "purpose": "meme-describe",
        // 会思考的模型先想再写（10-16 实测 cline-pass 的 deepseek：400 全被思考吃光，正文是空的）；不思考的写完就停，多给的用不上。
        "max_tokens": 4096,
        "messages": [
            {"role": "system", "text": system},
            {"role": "user", "text": format!("Language: {language}"), "images": [blob]},
        ],
    });
    let answer = core.call("model.call", &asked).map_err(specific)?;
    Ok(answer["text"].as_str().unwrap_or_default().to_string())
}

/// 核心拒了的，外层是笼统的一句（例如 `No model is available.`），具体原因在 `data.message`（没配看图模型、配的模型看不了
/// 图……，核心施工 X-23）：换成具体的那句往上交，页面才说得清为什么（项目主人 10-16 实测：只看得到笼统那句）。
fn specific(mut error: Value) -> Value {
    if let Some(detail) = error["data"]["message"].as_str().map(str::to_string) {
        error["message"] = Value::String(detail);
    }
    error
}

/// 照要的回一个 JSON（实测 10-16：只说「两行」时，中文的回答把行头也翻成了「第一行」「第二行」，第二行还接着描述）：取第一个
/// `{` 到最后一个 `}`，`description`、`usage` 都不空的才算。
fn json_of(text: &str) -> Option<(String, String)> {
    let (start, end) = (text.find('{')?, text.rfind('}')?);
    let value: Value = serde_json::from_str(text.get(start..=end)?).ok()?;
    let field = |key: &str| {
        value[key]
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    Some((field("description")?, field("usage")?))
}

/// 认得的行头（读不成 JSON 时退回照行读）：英文照要的写，中文、日文是模型把行头也照语言写了（实测 10-16：claude-haiku 写中文时去掉了英文行头）。
const DESCRIPTION: [&str; 4] = ["description", "描述", "说明", "説明"];
const USAGE: [&str; 4] = ["usage", "用法", "使い方", "使いどころ"];

/// 从回答里取描述、用法两行：带行头的照行头（冒号中英文都认、不分大小写、去掉 Markdown 的星号）；一个行头都没有的照先后取头两行
/// 不空的。少了一样的不算。
fn parse(text: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim().trim_start_matches(['*', '-', ' ']).trim())
        .filter(|line| !line.is_empty())
        .collect();
    let mut description = None;
    let mut usage = None;
    let mut labelled = false;
    for line in &lines {
        let Some((head, rest)) = line.split_once([':', '：']) else {
            continue;
        };
        let head = head.trim().trim_matches('*').to_lowercase();
        let rest = rest.trim().trim_matches('*').trim();
        if DESCRIPTION.contains(&head.as_str()) {
            labelled = true;
            description = (!rest.is_empty()).then(|| rest.to_string());
        } else if USAGE.contains(&head.as_str()) {
            labelled = true;
            usage = (!rest.is_empty()).then(|| rest.to_string());
        }
    }
    if !labelled && let [first, second, ..] = lines.as_slice() {
        description = Some((*first).to_string());
        usage = Some((*second).to_string());
    }
    Some((description?, usage?))
}

#[cfg(test)]
mod tests;
