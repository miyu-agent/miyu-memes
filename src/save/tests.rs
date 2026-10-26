//! 她存一张（施工 M-9）：聊天里的图经 im 取、带来历；描述先照看图模型、没配的照主模型；同一张不再请模型；她只给理由。

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
    /// 看图模型怎么答（主模型照答一份描述）。
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
            ("model.call", _) => Ok(
                json!({"text": r#"{"description": "猫说满意了吧。", "usage": "阴阳怪气的时候。"}"#}),
            ),
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
fn a_chat_image_is_fetched_from_im_described_and_kept_with_where_it_came_from() {
    let core = Fake::new(Ok(picture()), no_vision());
    let library = Library::in_memory().unwrap();
    let args = json!({"msg": "100000", "reason": "这只猫太会阴阳了"});
    let done = save(&library, &core, &args, &call(), 1 << 20).unwrap();
    assert_eq!(said(&done), "Saved eeeeeeee.", "{done}");
    assert_eq!(
        core.methods(),
        [
            "package.call",
            "blob.put",
            "model.call:vision",
            "model.call:chat"
        ],
        "看图模型做不成的退到主模型"
    );
    let asked = core.asked.lock().unwrap()[0].1.clone();
    assert_eq!(
        asked,
        json!({"package": "im", "method": "chat.media", "params": {"session": "s-1", "msg": "100000", "index": 1}})
    );
    let put = core.asked.lock().unwrap()[1].1.clone();
    assert_eq!(put["name"], "39685F13.jpg");
    assert_eq!(
        core.asked.lock().unwrap()[2].1["messages"][1]["text"],
        "Language: zh",
        "照理由的语言写"
    );
    let kept = library.get("eeeeeeee").unwrap().unwrap();
    assert_eq!(kept.description, "猫说满意了吧。");
    assert_eq!(kept.usage, "阴阳怪气的时候。");
    assert_eq!(kept.reason.as_deref(), Some("这只猫太会阴阳了"));
    assert_eq!(kept.source["kind"], "ai");
    assert_eq!(kept.source["venue"], "qq:group:960101");
    assert_eq!(kept.source["msg"], "100000");
    assert_eq!(kept.source["sender"]["name"], "inside_wang");
}

#[test]
fn a_configured_vision_model_writes_it_and_the_main_model_is_not_asked() {
    let vision = Ok(json!({"text": r#"{"description": "A cat.", "usage": "When smug."}"#}));
    let core = Fake::new(Ok(picture()), vision);
    let library = Library::in_memory().unwrap();
    let args = json!({"msg": 100_000, "index": 1, "reason": "Smug cat."});
    let done = save(&library, &core, &args, &call(), 1 << 20).unwrap();
    assert_eq!(said(&done), "Saved eeeeeeee.");
    assert_eq!(
        core.methods(),
        ["package.call", "blob.put", "model.call:vision"]
    );
    assert_eq!(
        library.get("eeeeeeee").unwrap().unwrap().description,
        "A cat."
    );
}

#[test]
fn one_already_kept_is_not_described_again() {
    let core = Fake::new(Ok(picture()), no_vision());
    let library = Library::in_memory().unwrap();
    let args = json!({"msg": "100000", "reason": "好"});
    save(&library, &core, &args, &call(), 1 << 20).unwrap();
    core.asked.lock().unwrap().clear();
    let again = save(&library, &core, &args, &call(), 1 << 20).unwrap();
    assert_eq!(said(&again), "Already saved as eeeeeeee.");
    assert_eq!(core.methods(), ["package.call", "blob.put"], "不再请模型");
}

#[test]
fn no_model_that_sees_images_means_nothing_is_kept() {
    struct Blind(Fake);
    impl Core for Blind {
        fn call(&self, method: &str, params: &Value) -> Answer {
            if method == "model.call" && params["use"] == "chat" {
                return Err(json!({"code": -32010, "message": "the chat model cannot see images"}));
            }
            self.0.call(method, params)
        }
    }
    let core = Blind(Fake::new(Ok(picture()), no_vision()));
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
        "Not saved, it could not be described: the chat model cannot see images"
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
