//! 她存一张（施工 M-9、M-9 补）：聊天里的图经 im 取、带来历；她看到了就自己写描述，没写的交看图模型写、顺便判断，不是表情包的
//! 不存，没有看图模型的不存；同一张不再请模型。

use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::{language, save};
use crate::calls::Core;
use crate::library::Library;
use crate::wire::Answer;

/// 假核心：照方法名答，记下问过的每一样。
struct Fake {
    /// im 的 `chat.media` 怎么答。
    media: Answer,
    /// 看图模型怎么答。
    vision: Answer,
    asked: Mutex<Vec<(String, Value)>>,
}

impl Fake {
    fn new(media: Answer, vision: Answer) -> Fake {
        Fake {
            media,
            vision,
            asked: Mutex::new(Vec::new()),
        }
    }

    /// 问过的方法名，`model.call` 带上用途。
    fn methods(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .map(|(method, params)| match params["use"].as_str() {
                Some(model_use) => format!("{method}:{model_use}"),
                None => method.clone(),
            })
            .collect()
    }
}

impl Core for Fake {
    fn call(&self, method: &str, params: &Value) -> Answer {
        self.asked
            .lock()
            .unwrap()
            .push((method.to_string(), params.clone()));
        match (method, params["use"].as_str()) {
            ("package.call", _) => self.media.clone(),
            ("blob.put", _) => Ok(json!({
                "blob": format!("sha256:{}", "e".repeat(64)), "kind": "image", "media_type": "image/jpeg",
                "width": 300, "height": 300
            })),
            ("model.call", Some("vision")) => self.vision.clone(),
            _ => Err(json!({"code": -32601})),
        }
    }
}

/// im 交回的一张图和它的来历。
fn picture() -> Value {
    json!({
        "kind": "sticker",
        "data": STANDARD.encode(b"jpeg"),
        "name": "39685F13.jpg",
        "origin": {
            "platform": "qq", "venue": "qq:group:960101", "msg": "100000",
            "sender": {"id": "qq:2703917070", "name": "inside_wang"}, "at": 1_761_465_600_000_i64
        }
    })
}

fn no_vision() -> Answer {
    Err(
        json!({"code": -32010, "message": "no vision model configured: set models.vision", "data": {"reason": "no_model"}}),
    )
}

fn call() -> Value {
    json!({"session": "s-1", "cwd": "/"})
}

fn said(done: &Value) -> &str {
    done["blocks"][0]["text"].as_str().unwrap()
}

#[test]
fn what_she_saw_she_describes_herself_and_no_model_is_asked() {
    let core = Fake::new(Ok(picture()), no_vision());
    let library = Library::in_memory().unwrap();
    let args = json!({"msg": "100000", "reason": "这只猫太会阴阳了",
        "description": "橘猫配字「满意了吧」。", "usage": "被逼着让步的时候。"});
    let done = save(&library, &core, &args, &call(), 1 << 20).unwrap();
    assert_eq!(said(&done), "Saved eeeeeeee.", "{done}");
    assert_eq!(
        core.methods(),
        ["package.call", "blob.put"],
        "她写了就不请模型"
    );
    let asked = core.asked.lock().unwrap()[0].1.clone();
    assert_eq!(
        asked,
        json!({"package": "miyu-im", "method": "chat.media", "params": {"session": "s-1", "msg": "100000", "index": 1}})
    );
    let put = core.asked.lock().unwrap()[1].1.clone();
    assert_eq!(put["name"], "39685F13.jpg");
    let kept = library.get("eeeeeeee").unwrap().unwrap();
    assert_eq!(kept.description, "橘猫配字「满意了吧」。");
    assert_eq!(kept.usage, "被逼着让步的时候。");
    assert_eq!(kept.reason.as_deref(), Some("这只猫太会阴阳了"));
    assert_eq!(kept.source["kind"], "ai");
    assert_eq!(kept.source["venue"], "qq:group:960101");
    assert_eq!(kept.source["msg"], "100000");
    assert_eq!(kept.source["sender"]["name"], "inside_wang");
}

#[test]
fn what_she_did_not_describe_the_vision_model_does_and_judges() {
    let vision = Ok(
        json!({"text": r#"{"sticker": true, "description": "猫说满意了吧。", "usage": "阴阳怪气的时候。"}"#}),
    );
    let core = Fake::new(Ok(picture()), vision);
    let library = Library::in_memory().unwrap();
    let args =
        json!({"msg": 100_000, "index": 1, "reason": "这只猫太会阴阳了", "usage": "只写了一样"});
    let done = save(&library, &core, &args, &call(), 1 << 20).unwrap();
    assert_eq!(said(&done), "Saved eeeeeeee.");
    assert_eq!(
        core.methods(),
        ["package.call", "blob.put", "model.call:vision"],
        "只写了一样的照没写，交看图模型"
    );
    let asked = core.asked.lock().unwrap()[2].1.clone();
    assert_eq!(
        asked["messages"][1]["text"], "Language: zh",
        "照理由的语言写"
    );
    assert!(
        asked["messages"][0]["text"]
            .as_str()
            .unwrap()
            .contains("sticker"),
        "顺便判断是不是表情包"
    );
    let kept = library.get("eeeeeeee").unwrap().unwrap();
    assert_eq!(
        (kept.description.as_str(), kept.usage.as_str()),
        ("猫说满意了吧。", "阴阳怪气的时候。")
    );
}

#[test]
fn what_the_vision_model_calls_no_sticker_is_not_kept() {
    let vision = Ok(
        json!({"text": r#"{"sticker": false, "description": "一张模型倍率表的截图。", "usage": "聊模型价格的时候。"}"#}),
    );
    let core = Fake::new(Ok(picture()), vision);
    let library = Library::in_memory().unwrap();
    let done = save(
        &library,
        &core,
        &json!({"msg": "1", "reason": "r"}),
        &call(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(done["error"], true);
    assert_eq!(
        said(&done),
        "Not saved: it is a screenshot, photo, document or chart, not a sticker."
    );
    assert!(library.all(false).unwrap().is_empty());
}

#[test]
fn one_already_kept_is_not_described_again() {
    let vision =
        Ok(json!({"text": r#"{"sticker": true, "description": "猫。", "usage": "好。"}"#}));
    let core = Fake::new(Ok(picture()), vision);
    let library = Library::in_memory().unwrap();
    let args = json!({"msg": "100000", "reason": "好"});
    save(&library, &core, &args, &call(), 1 << 20).unwrap();
    core.asked.lock().unwrap().clear();
    let again = save(&library, &core, &args, &call(), 1 << 20).unwrap();
    assert_eq!(said(&again), "Already saved as eeeeeeee.");
    assert_eq!(core.methods(), ["package.call", "blob.put"], "不再请模型");
}

#[test]
fn without_a_vision_model_what_she_did_not_describe_is_not_kept() {
    let core = Fake::new(Ok(picture()), no_vision());
    let library = Library::in_memory().unwrap();
    let done = save(
        &library,
        &core,
        &json!({"msg": "1", "reason": "r"}),
        &call(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(done["error"], true);
    assert_eq!(
        said(&done),
        "Not saved: no vision model could describe it (no vision model configured: set models.vision). Give description and usage if you have seen it."
    );
    assert_eq!(
        core.methods(),
        ["package.call", "blob.put", "model.call:vision"],
        "不再退到主模型"
    );
    assert!(library.all(false).unwrap().is_empty());
}

#[test]
fn im_refusals_come_back_as_sentences_she_can_act_on() {
    let refused =
        |reason: &str| Err(json!({"code": -32010, "data": {"code": -32010, "message": reason}}));
    for (reason, sentence) in [
        ("not_found", "No message 7 in this chat."),
        ("no_item", "Message 7 has no item #2."),
        ("not_image", "Item #2 of message 7 is not an image."),
        ("unfetched", "Cannot get that image from the chat."),
    ] {
        let core = Fake::new(refused(reason), no_vision());
        let library = Library::in_memory().unwrap();
        let args = json!({"msg": "7", "index": 2, "reason": "r"});
        let done = save(&library, &core, &args, &call(), 1 << 20).unwrap();
        assert_eq!((said(&done), &done["error"]), (sentence, &json!(true)));
        assert_eq!(core.methods(), ["package.call"], "{reason}");
    }
}

#[test]
fn missing_or_wrong_arguments_are_refused_before_anything_is_asked() {
    let core = Fake::new(Ok(picture()), no_vision());
    let library = Library::in_memory().unwrap();
    for (args, call, sentence) in [
        (
            json!({"msg": "1"}),
            call(),
            "Say why you keep it in reason.",
        ),
        (
            json!({"reason": "r"}),
            call(),
            "Give path, or msg and index.",
        ),
        (
            json!({"msg": "1", "index": 0, "reason": "r"}),
            call(),
            "index counts from 1.",
        ),
        (
            json!({"msg": "1", "reason": "r"}),
            json!({"cwd": "/"}),
            "msg works only in a chat.",
        ),
    ] {
        let done = save(&library, &core, &args, &call, 1 << 20).unwrap();
        assert_eq!(said(&done), sentence, "{args}");
    }
    assert!(core.methods().is_empty());
}

#[test]
fn the_description_language_follows_the_reason() {
    assert_eq!(language("这只猫太会阴阳了"), "zh");
    assert_eq!(language("このネコかわいい"), "ja");
    assert_eq!(language("Smug cat."), "en");
}

#[test]
fn a_message_number_copied_with_its_label_is_read_bare() {
    // 施工 M-11：她照看到的 `[msg=…]` 抄成 `msg-100012` 这类，照编号认；空的、只有外壳的不认。
    for written in [
        "100012",
        " 100012 ",
        "msg=100012",
        "msg-100012",
        "MSG:100012",
        "[msg=100012]",
    ] {
        assert_eq!(
            super::Wanted::read(&json!({"msg": written})),
            Some(Ok(super::Wanted {
                msg: "100012".to_string(),
                index: 1
            })),
            "{written}"
        );
    }
    assert_eq!(
        super::Wanted::read(&json!({"msg": 100_012, "index": 2})),
        Some(Ok(super::Wanted {
            msg: "100012".to_string(),
            index: 2
        }))
    );
    for empty in ["", "  ", "msg=", "[]"] {
        assert_eq!(
            super::Wanted::read(&json!({"msg": empty})),
            None,
            "{empty:?}"
        );
    }
    assert_eq!(
        super::Wanted::read(&json!({"msg": "message7"})).map(|w| w.map(|w| w.msg)),
        Some(Ok("message7".to_string())),
        "不是 msg 开头加分隔的照原样"
    );
}
