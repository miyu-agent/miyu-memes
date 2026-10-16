use std::sync::Mutex;

use serde_json::{Value, json};

use super::{describe, parse};
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
        reply: Ok(json!({"text": "Description: 一只猫歪头。\nUsage: 卖萌的时候。"})),
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
}
