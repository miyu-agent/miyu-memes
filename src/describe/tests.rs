use std::sync::Mutex;

use serde_json::{Value, json};

use super::{describe, json_of, parse};
use crate::calls::Core;
use crate::wire::Answer;

/// 假核心：记下问的什么，照 `reply` 答。
struct Fake {
    reply: Answer,
    asked: Mutex<Vec<Value>>,
}

impl Core for Fake {
    fn call(&self, method: &str, params: &Value) -> Answer {
        assert_eq!(method, "model.call");
        self.asked.lock().unwrap().push(params.clone());
        self.reply.clone()
    }
}

#[test]
fn it_asks_the_configured_vision_model_with_the_image_and_reads_two_lines() {
    let core = Fake {
        reply: Ok(
            json!({"text": "```json\n{\"description\": \"一只猫歪头。\", \"usage\": \"卖萌的时候。\"}\n```"}),
        ),
        asked: Mutex::new(Vec::new()),
    };
    let got = describe(&core, "sha256:aa", "zh").unwrap();
    assert_eq!(
        got,
        ("一只猫歪头。".to_string(), "卖萌的时候。".to_string())
    );
    let asked = &core.asked.lock().unwrap()[0];
    assert_eq!(asked["use"], "vision", "照人配的看图模型");
    assert!(asked.get("model").is_none());
    assert_eq!(asked["messages"][1]["images"], json!(["sha256:aa"]));
    assert_eq!(asked["messages"][1]["text"], "Language: zh");
}

#[test]
fn refusals_pass_through_and_a_reply_without_both_lines_is_an_error() {
    let refused = Fake {
        reply: Err(json!({"code": -32010, "data": {"reason": "no_model"}})),
        asked: Mutex::new(Vec::new()),
    };
    assert_eq!(
        describe(&refused, "sha256:aa", "en").unwrap_err()["data"]["reason"],
        "no_model"
    );
    let half = Fake {
        reply: Ok(json!({"text": "Description: a cat"})),
        asked: Mutex::new(Vec::new()),
    };
    assert_eq!(
        describe(&half, "sha256:aa", "en").unwrap_err()["code"],
        -32000
    );
}

#[test]
fn the_two_lines_are_read_loosely() {
    let text = "**Description:** A cat.\n- usage：When greeting.\nExtra: no";
    assert_eq!(
        parse(text),
        Some(("A cat.".to_string(), "When greeting.".to_string()))
    );
    assert_eq!(parse("Description:\nUsage: x"), None, "空的那一行不算");
    assert_eq!(
        parse("描述：一只猫。\n用法：打招呼。"),
        Some(("一只猫。".to_string(), "打招呼。".to_string()))
    );
    // 实测 10-16：模型照中文写、把行头也去掉了，照先后取头两行。
    let bare = "一个黑色剪影的卡通人物举着「群主」牌子。\n\n适合在群聊里开玩笑时使用。";
    assert_eq!(
        parse(bare),
        Some((
            "一个黑色剪影的卡通人物举着「群主」牌子。".to_string(),
            "适合在群聊里开玩笑时使用。".to_string()
        ))
    );
    assert_eq!(parse("只有一行"), None);
}

#[test]
fn a_json_reply_is_read_first_and_lines_are_the_fallback() {
    assert_eq!(
        json_of(r#"Sure: {"description": "A cat.", "usage": "Hi."}"#),
        Some(("A cat.".to_string(), "Hi.".to_string()))
    );
    assert_eq!(
        json_of(r#"{"description": "A cat.", "usage": " "}"#),
        None,
        "空的不算"
    );
    assert_eq!(
        json_of("Description: a\nUsage: b"),
        None,
        "不是 JSON 的交给照行读"
    );
}

#[test]
fn a_refusal_carries_the_specific_reason_not_the_generic_line() {
    let core = Fake {
        reply: Err(json!({
            "code": -32010,
            "message": "No model is available.",
            "data": {"reason": "no_model", "message": "model \"dev/x\" does not take images"}
        })),
        asked: Mutex::new(Vec::new()),
    };
    let error = describe(&core, "sha256:aa", "zh").unwrap_err();
    assert_eq!(error["message"], "model \"dev/x\" does not take images");
    assert_eq!(error["data"]["reason"], "no_model", "别的格不动");
    let plain = Fake {
        reply: Err(json!({"code": -32000, "message": "busy"})),
        asked: Mutex::new(Vec::new()),
    };
    assert_eq!(
        describe(&plain, "sha256:aa", "zh").unwrap_err()["message"],
        "busy",
        "没有具体原因的照原话"
    );
}
