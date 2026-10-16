use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::{Core, State, tool};
use crate::library::Library;
use crate::settings::Settings;
use crate::wire::Answer;

/// 假的核心：`blob.put` 照文件交一张 2×2 的 png，`model.embed` 当没配。
struct Fake;

impl Core for Fake {
    fn call(&self, method: &str, params: &Value) -> Answer {
        match method {
            "blob.put"
                if params["path"].as_str().is_some_and(|path| {
                    std::path::Path::new(path)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                }) =>
            {
                Ok(json!({
                    "blob": format!("sha256:{}", "c".repeat(64)), "kind": "image", "media_type": "image/png",
                    "width": 2, "height": 2
                }))
            }
            "blob.put" => Ok(json!({"blob": format!("sha256:{}", "d".repeat(64)), "kind": "file"})),
            _ => Err(json!({"code": -32010, "data": {"reason": "embedding_off"}})),
        }
    }
}

fn state() -> State {
    State {
        library: Mutex::new(Library::in_memory().unwrap()),
        settings: Mutex::new(Settings::default()),
        core: Arc::new(Fake),
    }
}

fn call(state: &State, name: &str, args: &Value, cwd: &str) -> Value {
    tool(state, &json!({"tool": name, "args": args, "cwd": cwd}))
}

fn said(done: &Value) -> &str {
    done["blocks"][0]["text"].as_str().unwrap()
}

#[test]
fn save_then_send_ends_the_turn_and_carries_the_image() {
    let dir = std::env::temp_dir().join(format!("miyu-memes-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("cat.png"), b"png").unwrap();
    let state = state();
    let cwd = dir.to_string_lossy().into_owned();
    let saved = call(
        &state,
        "meme_save",
        &json!({"path": "cat.png", "description": "A cat.", "usage": "Hi."}),
        &cwd,
    );
    assert_eq!(said(&saved), "Saved cccccccc.");
    let again = call(
        &state,
        "meme_save",
        &json!({"path": "cat.png", "description": "x", "usage": "y"}),
        &cwd,
    );
    assert_eq!(said(&again), "Already saved as cccccccc.");
    let sent = call(&state, "meme_send", &json!({"id": "cccccccc"}), &cwd);
    assert_eq!(said(&sent), "Sent cccccccc.");
    assert_eq!(sent["end_turn"], true, "默认发完就结束这一轮");
    assert_eq!(sent["sent"][0]["sticker"], true);
    assert_eq!(sent["sent"][0]["alt"], "A cat.");
    assert_eq!(sent["sent"][0]["width"], 2);
    let keep = call(
        &state,
        "meme_send",
        &json!({"id": "cccccccc", "end": false}),
        &cwd,
    );
    assert!(keep.get("end_turn").is_none(), "end 写假的接着说");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn bad_inputs_come_back_as_errors_she_can_read() {
    let state = state();
    let missing = call(&state, "meme_send", &json!({"id": "nope"}), "/");
    assert_eq!(missing["error"], true);
    assert!(said(&missing).contains("meme_search"));
    let not_image =
        std::env::temp_dir().join(format!("miyu-memes-test-{}.txt", std::process::id()));
    std::fs::write(&not_image, b"text").unwrap();
    let file = call(
        &state,
        "meme_save",
        &json!({"path": not_image.to_string_lossy(), "description": "a", "usage": "b"}),
        "/",
    );
    assert_eq!(said(&file), "That file is not an image.");
    std::fs::remove_file(&not_image).unwrap();
    assert_eq!(
        call(&state, "meme_edit", &json!({"id": "x"}), "/")["error"],
        true
    );
    assert_eq!(
        said(&call(&state, "meme_delete", &json!({"id": "x"}), "/")),
        "No sticker x."
    );
    let empty = call(&state, "meme_search", &json!({"query": "猫"}), "/");
    assert!(
        said(&empty).starts_with("No sticker fits"),
        "embedding 关着也照关键词找"
    );
}

#[test]
fn files_over_the_size_limit_are_refused() {
    let state = state();
    state.settings.lock().unwrap().max_bytes = 2;
    let big = std::env::temp_dir().join(format!("miyu-memes-big-{}.png", std::process::id()));
    std::fs::write(&big, b"12345").unwrap();
    let done = call(
        &state,
        "meme_save",
        &json!({"path": big.to_string_lossy(), "description": "a", "usage": "b"}),
        "/",
    );
    assert!(said(&done).starts_with("The file is larger than"));
    std::fs::remove_file(&big).unwrap();
}
