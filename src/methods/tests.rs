use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::call;
use crate::calls::{Core, State};
use crate::library::Library;
use crate::settings::Settings;
use crate::wire::Answer;

struct Off;

/// 只答 `blob.put`、`blob.get` 的核心。
struct Blobs;

impl Core for Blobs {
    fn call(&self, method: &str, _: &Value) -> Answer {
        match method {
            "blob.put" => Ok(
                json!({"blob": format!("sha256:{}", "c".repeat(64)), "media_type": "image/png",
                                    "width": 8, "height": 9, "kind": "image"}),
            ),
            "blob.get" => Ok(json!({"data": "iVBO", "size": 3})),
            _ => Err(json!({"code": -32601})),
        }
    }
}

impl Core for Off {
    fn call(&self, _: &str, _: &Value) -> Answer {
        Err(json!({"code": -32010}))
    }
}

fn state() -> State {
    State {
        library: Mutex::new(Library::in_memory().unwrap()),
        settings: Mutex::new(Settings::default()),
        core: Arc::new(Off),
    }
}

fn image(extra: &Value) -> Value {
    let mut params = json!({"blob": format!("sha256:{}", "e".repeat(64)), "media_type": "image/gif",
                            "width": 3, "height": 4, "description": "A dog.", "usage": "Sleepy."});
    for (key, value) in extra.as_object().unwrap() {
        params[key] = value.clone();
    }
    params
}

#[test]
fn another_package_adds_with_its_name_and_origin_and_people_upload_as_human() {
    let state = state();
    let added = call(
        &state,
        &json!({"method": "add", "from": "onebot", "params": image(&json!({"reason": "funny", "origin": {"platform": "qq", "kind": "spoof"}}))}),
    )
    .unwrap();
    assert_eq!(added, json!({"id": "eeeeeeee", "created": true}));
    let got = call(
        &state,
        &json!({"method": "get", "params": {"id": "eeeeeeee"}}),
    )
    .unwrap();
    let source = &got["meme"]["source"];
    assert_eq!(source["kind"], "package", "调的一方冒充不了来源的种类");
    assert_eq!(source["package"], "onebot");
    assert_eq!(source["platform"], "qq");
    assert_eq!(got["meme"]["reason"], "funny");
    let human = call(&state, &json!({"method": "add", "params": image(&json!({"blob": format!("sha256:{}", "f".repeat(64))}))})).unwrap();
    let id = human["id"].as_str().unwrap();
    let got = call(&state, &json!({"method": "get", "params": {"id": id}})).unwrap();
    assert_eq!(got["meme"]["source"]["kind"], "human");
}

#[test]
fn the_admin_page_lists_disables_restores_and_purges() {
    let state = state();
    call(
        &state,
        &json!({"method": "add", "params": image(&json!({}))}),
    )
    .unwrap();
    assert_eq!(
        call(
            &state,
            &json!({"method": "delete", "params": {"id": "eeeeeeee"}})
        )
        .unwrap()["found"],
        true
    );
    let listed = call(&state, &json!({"method": "list"})).unwrap();
    assert_eq!(listed["memes"][0]["enabled"], false, "停用的后台页照样列");
    call(
        &state,
        &json!({"method": "restore", "params": {"id": "eeeeeeee"}}),
    )
    .unwrap();
    call(
        &state,
        &json!({"method": "update", "params": {"id": "eeeeeeee", "usage": "Tired."}}),
    )
    .unwrap();
    let found = call(
        &state,
        &json!({"method": "search", "params": {"query": "Tired"}}),
    )
    .unwrap();
    assert_eq!(found["memes"][0]["id"], "eeeeeeee");
    assert_eq!(
        call(
            &state,
            &json!({"method": "purge", "params": {"id": "eeeeeeee"}})
        )
        .unwrap()["found"],
        true
    );
    assert_eq!(
        call(&state, &json!({"method": "list"})).unwrap()["memes"],
        json!([])
    );
}

#[test]
fn bad_params_and_unknown_methods_are_refused() {
    let state = state();
    assert_eq!(
        call(&state, &json!({"method": "add", "params": {"blob": "x"}})).unwrap_err()["code"],
        -32602
    );
    assert_eq!(
        call(&state, &json!({"method": "get", "params": {}})).unwrap_err()["code"],
        -32602
    );
    assert_eq!(
        call(&state, &json!({"method": "nope"})).unwrap_err()["code"],
        -32601
    );
}

#[test]
fn the_page_uploads_base64_as_human_and_reads_images_back() {
    let mut state = state();
    state.core = Arc::new(Blobs);
    let params = json!({"data": "iVBO", "name": "a.png", "description": "A cat.", "usage": "Hi."});
    let added = call(&state, &json!({"method": "upload", "params": params})).unwrap();
    assert_eq!(added, json!({"id": "cccccccc", "created": true}));
    let got = call(
        &state,
        &json!({"method": "get", "params": {"id": "cccccccc"}}),
    )
    .unwrap();
    assert_eq!(got["meme"]["source"]["kind"], "human");
    assert_eq!(got["meme"]["width"], 8);
    let image = call(
        &state,
        &json!({"method": "image", "params": {"id": "cccccccc"}}),
    )
    .unwrap();
    assert_eq!(image, json!({"media_type": "image/png", "data": "iVBO"}));
    let missing = call(
        &state,
        &json!({"method": "image", "params": {"id": "nope"}}),
    );
    assert_eq!(missing.unwrap_err()["code"], -32000);
}
