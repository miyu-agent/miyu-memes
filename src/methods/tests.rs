use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::call;
use crate::calls::{Core, State};
use crate::library::Library;
use crate::settings::Settings;
use crate::thumb::tests::{noisy, scratch};
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
    assert_eq!(
        image,
        json!({"media_type": "image/png", "data": "iVBO", "size": 3}),
        "小图不带 offset：和原来一样，多一格 size，没有 next"
    );
    let missing = call(
        &state,
        &json!({"method": "image", "params": {"id": "nope"}}),
    );
    assert_eq!(missing.unwrap_err()["code"], -32000);
}

/// 答 `model.embed`、`model.call` 的核心：向量照条数给，看图的照一句写好的答。
struct Thinking;

impl Core for Thinking {
    fn call(&self, method: &str, params: &Value) -> Answer {
        match method {
            "model.embed" => {
                let n = params["texts"].as_array().map_or(0, Vec::len);
                Ok(json!({"model": "local:test", "vectors": vec![vec![1.0, 0.0]; n]}))
            }
            "model.call" => {
                assert_eq!(params["use"], "vision");
                assert_eq!(params["messages"][1]["text"], "Language: zh");
                Ok(json!({"text": "Description: 一只狗。\nUsage: 困的时候。"}))
            }
            _ => Err(json!({"code": -32601})),
        }
    }
}

#[test]
fn the_page_rebuilds_the_index_and_asks_for_a_new_description_without_saving_it() {
    let mut state = state();
    state.core = Arc::new(Thinking);
    call(
        &state,
        &json!({"method": "add", "params": image(&json!({}))}),
    )
    .unwrap();
    let rebuilt = call(&state, &json!({"method": "reindex"})).unwrap();
    assert_eq!(rebuilt, json!({"model": "local:test", "count": 1}));
    let described = call(
        &state,
        &json!({"method": "describe", "params": {"id": "eeeeeeee", "language": "zh"}}),
    )
    .unwrap();
    assert_eq!(
        described,
        json!({"description": "一只狗。", "usage": "困的时候。"})
    );
    let got = call(
        &state,
        &json!({"method": "get", "params": {"id": "eeeeeeee"}}),
    )
    .unwrap();
    assert_eq!(
        got["meme"]["description"], "A dog.",
        "写好的不存，人看了再存"
    );
    let missing = call(
        &state,
        &json!({"method": "describe", "params": {"id": "nope"}}),
    );
    assert_eq!(missing.unwrap_err()["message"], "no such sticker");
}

#[test]
fn rebuilding_without_embedding_is_embedding_off() {
    let state = state();
    call(
        &state,
        &json!({"method": "add", "params": image(&json!({}))}),
    )
    .unwrap();
    let off = call(&state, &json!({"method": "reindex"})).unwrap_err();
    assert_eq!(off["message"], "embedding_off");
}

/// 交回一张真图的核心：`blob.get` 照 `offset`、`length` 给，记下读了几次。
struct Pictures {
    bytes: Vec<u8>,
    reads: Mutex<usize>,
}

impl Core for Pictures {
    fn call(&self, method: &str, params: &Value) -> Answer {
        if method != "blob.get" {
            return Err(json!({"code": -32601}));
        }
        *self.reads.lock().unwrap() += 1;
        let from = usize::try_from(params["offset"].as_u64().unwrap()).unwrap();
        let to = (from + usize::try_from(params["length"].as_u64().unwrap()).unwrap())
            .min(self.bytes.len());
        Ok(
            json!({"data": STANDARD.encode(&self.bytes[from.min(to)..to]), "size": self.bytes.len()}),
        )
    }
}

#[test]
fn the_page_gets_thumbs_and_chunked_originals_and_a_purge_drops_the_cached_thumb() {
    let dir = scratch("methods-thumb");
    let core = Arc::new(Pictures {
        bytes: noisy(700, 700, false),
        reads: Mutex::new(0),
    });
    let state = State {
        library: Mutex::new(Library::open(&dir.join("library.db")).unwrap()),
        settings: Mutex::new(Settings::default()),
        core: Arc::clone(&core) as Arc<dyn Core>,
    };
    call(
        &state,
        &json!({"method": "add", "params": image(&json!({}))}),
    )
    .unwrap();
    let thumb = call(
        &state,
        &json!({"method": "thumb", "params": {"id": "eeeeeeee"}}),
    )
    .unwrap();
    assert_eq!(thumb["media_type"], "image/jpeg");
    assert!(thumb["data"].as_str().unwrap().len() <= 256 * 1024);
    let cached = dir.join("thumbs").join("e".repeat(64));
    assert!(cached.is_file(), "小图缓存在库旁边的 thumbs/，文件名照哈希");
    // 原图：照 next 一块一块取，接起来就是整张的 base64。
    let (mut joined, mut offset) = (String::new(), Some(0));
    while let Some(at) = offset {
        let piece = call(
            &state,
            &json!({"method": "image", "params": {"id": "eeeeeeee", "offset": at}}),
        )
        .unwrap();
        assert_eq!(piece["size"], core.bytes.len());
        joined.push_str(piece["data"].as_str().unwrap());
        offset = piece["next"].as_u64();
    }
    assert_eq!(STANDARD.decode(joined).unwrap(), core.bytes);
    let bad = call(
        &state,
        &json!({"method": "image", "params": {"id": "eeeeeeee", "offset": "0"}}),
    );
    assert_eq!(bad.unwrap_err()["code"], -32602);
    call(
        &state,
        &json!({"method": "purge", "params": {"id": "eeeeeeee"}}),
    )
    .unwrap();
    assert!(!cached.exists(), "删了表情，小图跟着删");
    drop(state);
    drop(std::fs::remove_dir_all(dir));
}

#[test]
fn every_page_method_is_registered() {
    let names: Vec<String> = super::register()["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|one| one["name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"thumb".to_string()) && names.contains(&"image".to_string()));
}

#[test]
fn the_bus_name_memes_reaches_the_same_add_and_search() {
    // 施工 M-15（核心设计 67，BUS-4）：总线上提供名字 `memes`，核心照 `memes.add`、`memes.search` 转来；不带前缀的照旧登记着
    // （后台页、人的连接用）。两种叫法是同一件事：回应一样，来源照 `from` 记。
    let (plain, prefixed) = (state(), state());
    let params = image(&json!({"reason": "funny", "origin": {"platform": "qq"}}));
    let ask = |state: &State, method: &str, params: &Value| {
        call(
            state,
            &json!({"method": method, "from": "im", "params": params}),
        )
    };
    let added = ask(&prefixed, "memes.add", &params).expect("放得进");
    assert_eq!(added, json!({"id": "eeeeeeee", "created": true}));
    assert_eq!(ask(&plain, "add", &params).expect("放得进"), added);
    let got = |state: &State| {
        let got = call(
            state,
            &json!({"method": "get", "params": {"id": "eeeeeeee"}}),
        )
        .unwrap();
        got["meme"]["source"].clone()
    };
    assert_eq!(got(&prefixed)["package"], "im", "来源照 from 记");
    assert_eq!(got(&prefixed), got(&plain));
    let query = json!({"query": "Sleepy", "limit": 3});
    let found = ask(&prefixed, "memes.search", &query).expect("找得到");
    assert_eq!(found["memes"][0]["id"], "eeeeeeee");
    assert_eq!(ask(&prefixed, "search", &query).expect("找得到"), found);
    // 登记的名字：原来的十三个照旧，多带前缀的两个。
    let registered = super::register();
    let names: Vec<&str> = registered["methods"]
        .as_array()
        .expect("是数组")
        .iter()
        .filter_map(|one| one["name"].as_str())
        .collect();
    assert_eq!(names.len(), 15, "{names:?}");
    for name in ["add", "search", "memes.add", "memes.search"] {
        assert!(names.contains(&name), "{name}");
    }
    // 清单里声明了名字、导出的是带前缀的两个（核心照它认 `bus.call` 调不调得到）。
    let manifest = include_str!("../../package/package.toml");
    assert!(manifest.contains("provides = [\"memes\"]"), "{manifest}");
    assert!(
        manifest.contains("methods = [\"memes.add\", \"memes.search\"]"),
        "{manifest}"
    );
}
