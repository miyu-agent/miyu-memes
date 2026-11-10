use std::path::Path;
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::{PIECE, image, store, thumb};
use crate::calls::Core;
use crate::library::{Library, Meme, NewMeme};
use crate::thumb::tests::{noisy, scratch};
use crate::thumb::{MOST, SIDE};
use crate::wire::Answer;

/// 核心收一行最多多少字节：包回的一条回应也是一行。
const LINE_LIMIT: usize = 1024 * 1024;

/// 假核心：记下调了什么；`blob.get` 照 `offset`、`length` 交回 `bytes` 的一段（协议上一块最多 512 KiB），`blob.put` 交回 `put`。
struct Fake {
    bytes: Vec<u8>,
    put: Value,
    asked: Mutex<Vec<(String, Value)>>,
}

impl Core for Fake {
    fn call(&self, method: &str, params: &Value) -> Answer {
        self.asked
            .lock()
            .unwrap()
            .push((method.to_string(), params.clone()));
        match method {
            "blob.get" => {
                let length = usize::try_from(params["length"].as_u64().unwrap()).unwrap();
                assert!(length <= 512 * 1024, "一块最多 512 KiB");
                let from = usize::try_from(params["offset"].as_u64().unwrap()).unwrap();
                let to = (from + length).min(self.bytes.len());
                Ok(
                    json!({"data": STANDARD.encode(&self.bytes[from.min(to)..to]), "size": self.bytes.len()}),
                )
            }
            "blob.put" => Ok(self.put.clone()),
            _ => Err(json!({"code": -32601})),
        }
    }
}

impl Fake {
    fn reads(&self) -> usize {
        let asked = self.asked.lock().unwrap();
        asked.iter().filter(|(one, _)| one == "blob.get").count()
    }
}

fn fake(bytes: Vec<u8>, put: Value) -> Fake {
    Fake {
        bytes,
        put,
        asked: Mutex::new(Vec::new()),
    }
}

/// 一份假图：`len` 个字节，照位置变，拼错了看得出来。
fn picture(len: usize) -> Vec<u8> {
    (0..len).map(|at| u8::try_from(at % 251).unwrap()).collect()
}

fn meme() -> Meme {
    let library = Library::in_memory().unwrap();
    let meme = NewMeme {
        hash: format!("sha256:{}", "a".repeat(64)),
        media_type: "image/png".into(),
        width: 1,
        height: 1,
        description: "A dot.".into(),
        usage: "Never.".into(),
        source: json!({"kind": "human"}),
        reason: None,
    };
    library.add(&meme, 1).unwrap();
    library.get("aaaaaaaa").unwrap().unwrap()
}

/// 一条回应写成一行有多长。
fn line(answer: &Value) -> usize {
    json!({"id": "m-1", "result": answer}).to_string().len()
}

#[test]
fn a_big_original_comes_in_pieces_that_join_into_the_whole_base64() {
    let size = usize::try_from(PIECE).unwrap() * 2 + 5;
    let core = fake(picture(size), Value::Null);
    let meme = meme();
    let (mut joined, mut offset, mut pieces) = (String::new(), Some(0), 0);
    while let Some(at) = offset {
        let got = image(&core, &meme, at).unwrap();
        assert!(line(&got) < LINE_LIMIT, "每一条回应都在一行 1 MiB 以内");
        assert_eq!(got["media_type"], "image/png");
        assert_eq!(got["size"], size, "size 是整张的字节数");
        joined.push_str(got["data"].as_str().unwrap());
        offset = got["next"].as_u64();
        pieces += 1;
        assert!(
            offset.is_none_or(|next| next == at + PIECE),
            "next 是下一块的 offset"
        );
    }
    assert_eq!(pieces, 3);
    assert_eq!(
        joined,
        STANDARD.encode(&core.bytes),
        "接起来就是整张的 base64"
    );
    assert_eq!(core.reads(), 3, "一次只读要的那一块");
}

#[test]
fn a_small_original_without_offset_answers_as_before_plus_size() {
    let core = fake(picture(1000), Value::Null);
    let got = image(&core, &meme(), 0).unwrap();
    assert_eq!(
        got,
        json!({"media_type": "image/png", "data": STANDARD.encode(&core.bytes), "size": 1000}),
        "没有 next"
    );
    let past = image(&core, &meme(), 3000).unwrap();
    assert_eq!(past["data"], "", "过了头的是空的一块");
    assert!(past.get("next").is_none());
}

#[test]
fn a_thumb_is_small_fits_a_line_and_is_cached() {
    let dir = scratch("images-thumb");
    let core = fake(noisy(1200, 900, false), Value::Null);
    assert!(core.bytes.len() > LINE_LIMIT, "原图自己一行放不下");
    let got = thumb(&core, &meme(), Some(&dir)).unwrap();
    assert_eq!(got["media_type"], "image/jpeg");
    let data = got["data"].as_str().unwrap();
    assert!(data.len() <= MOST / 3 * 4 && line(&got) < LINE_LIMIT / 2);
    let small = image::load_from_memory(&STANDARD.decode(data).unwrap()).unwrap();
    assert_eq!(small.width().max(small.height()), SIDE);
    let first = core.reads();
    assert!(first >= 3, "原图分块读回来");
    assert_eq!(thumb(&core, &meme(), Some(&dir)).unwrap(), got);
    assert_eq!(core.reads(), first, "缓存里有的不再读原图");
    drop(std::fs::remove_dir_all(dir));
}

#[test]
fn a_broken_cached_thumb_is_made_again() {
    let dir = scratch("images-thumb-broken");
    let core = fake(noisy(400, 300, false), Value::Null);
    let got = thumb(&core, &meme(), Some(&dir)).unwrap();
    let file = dir.join("a".repeat(64));
    std::fs::write(&file, b"garbage").unwrap();
    let first = core.reads();
    assert_eq!(
        thumb(&core, &meme(), Some(&dir)).unwrap(),
        got,
        "重做的一样"
    );
    assert!(core.reads() > first, "坏了的重读原图");
    assert_ne!(std::fs::read(&file).unwrap(), b"garbage", "缓存换成好的");
    drop(std::fs::remove_dir_all(dir));
}

#[test]
fn a_thumb_without_a_cache_dir_or_of_a_broken_original() {
    let core = fake(noisy(300, 300, false), Value::Null);
    let none: Option<&Path> = None;
    assert_eq!(
        thumb(&core, &meme(), none).unwrap()["media_type"],
        "image/jpeg"
    );
    let broken = fake(picture(5000), Value::Null);
    assert_eq!(
        thumb(&broken, &meme(), none).unwrap_err()["message"],
        "not_image"
    );
}

#[test]
fn an_upload_is_put_into_the_core_and_refused_when_not_an_image() {
    let put = json!({"blob": "sha256:b", "media_type": "image/gif", "width": 4, "height": 5, "kind": "image"});
    let core = fake(Vec::new(), put);
    let params = json!({"data": "AAAA", "name": "a.gif", "description": "d", "usage": "u"});
    let merged = store(&core, &params, 3).unwrap();
    assert_eq!(merged["blob"], "sha256:b");
    assert_eq!(merged["width"], 4);
    assert_eq!(merged["usage"], "u");
    assert_eq!(
        core.asked.lock().unwrap()[0].1,
        json!({"data": "AAAA", "name": "a.gif"})
    );
    assert_eq!(
        store(&core, &params, 2).unwrap_err()["message"],
        "not_image",
        "超了要缩，解不开的就是不是图"
    );
    let text = fake(Vec::new(), json!({"blob": "sha256:c", "kind": "text"}));
    assert_eq!(
        store(&text, &params, 3).unwrap_err()["message"],
        "not_image"
    );
}
