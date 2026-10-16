use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::{Core, State, tool};
use crate::library::Library;
use crate::settings::Settings;
use crate::wire::Answer;

/// 假的核心：`blob.put` 照文件名（`path` 或缩过以后交的 `name`）认图，图交一张 2×2 的，`model.embed` 当没配；缩过的那一份
/// 记在 `PUT_DATA` 里。
struct Fake;

static PUT_DATA: Mutex<Option<(String, usize)>> = Mutex::new(None);

impl Core for Fake {
    fn call(&self, method: &str, params: &Value) -> Answer {
        let name = params["path"]
            .as_str()
            .or(params["name"].as_str())
            .unwrap_or_default();
        let image = std::path::Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("jpg"));
        if let Some(data) = params["data"].as_str() {
            *PUT_DATA.lock().unwrap() = Some((name.to_string(), data.len()));
        }
        match method {
            "blob.put" if image => Ok(json!({
                "blob": format!("sha256:{}", "c".repeat(64)), "kind": "image", "media_type": "image/png",
                "width": 2, "height": 2
            })),
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
fn files_over_the_size_limit_are_shrunk_and_junk_is_refused() {
    let state = state();
    state.settings.lock().unwrap().max_bytes = 20 * 1024;
    let dir = std::env::temp_dir().join(format!("miyu-memes-big-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 一张 400×300 的噪点 PNG：远超 20 KiB，缩成 JPEG 才装得下。
    let mut seed = 7_u32;
    let picture = image::RgbImage::from_fn(400, 300, |_, _| {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let [a, b, c, _] = seed.to_le_bytes();
        image::Rgb([a, b, c])
    });
    let big = dir.join("big.png");
    picture.save(&big).unwrap();
    assert!(std::fs::metadata(&big).unwrap().len() > 20 * 1024);
    let done = call(
        &state,
        "meme_save",
        &json!({"path": big.to_string_lossy(), "description": "a", "usage": "b"}),
        "/",
    );
    assert!(said(&done).starts_with("Saved"), "{done}");
    let (name, length) = PUT_DATA.lock().unwrap().clone().unwrap();
    assert_eq!(name, "big.jpg", "缩过的照新格式改扩展名");
    assert!(length as u64 <= 20 * 1024 * 4 / 3 + 4, "交给核心的是缩过的");
    let junk = dir.join("junk.png");
    std::fs::write(&junk, vec![7_u8; 30 * 1024]).unwrap();
    let refused = call(
        &state,
        "meme_save",
        &json!({"path": junk.to_string_lossy(), "description": "a", "usage": "b"}),
        "/",
    );
    assert_eq!(said(&refused), "That file is not an image.");
    std::fs::remove_dir_all(&dir).unwrap();
}
