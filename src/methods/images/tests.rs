use std::sync::Mutex;

use serde_json::{Value, json};

use super::{CHUNK, decoded_len, image, store};
use crate::calls::Core;
use crate::library::{Library, NewMeme};
use crate::wire::Answer;

/// 假核心：记下调了什么；`blob.get` 照 `size` 交回 `CHUNK` 大小的块（base64 的字照长度造），`blob.put` 交回 `put`。
struct Fake {
    size: u64,
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
                let offset = params["offset"].as_u64().unwrap();
                let bytes = self.size.saturating_sub(offset).min(CHUNK);
                let mut text = "A".repeat(usize::try_from(bytes / 3 * 4).unwrap());
                match bytes % 3 {
                    1 => text.push_str("AA=="),
                    2 => text.push_str("AAA="),
                    _ => {}
                }
                Ok(json!({"data": text, "size": self.size}))
            }
            "blob.put" => Ok(self.put.clone()),
            _ => Err(json!({"code": -32601})),
        }
    }
}

fn fake(size: u64, put: Value) -> Fake {
    Fake {
        size,
        put,
        asked: Mutex::new(Vec::new()),
    }
}

fn library() -> Library {
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
    library
}

#[test]
fn a_big_image_comes_back_whole_in_chunks_that_join_cleanly() {
    let size = CHUNK * 2 + 5;
    let core = fake(size, Value::Null);
    let meme = library().get("aaaaaaaa").unwrap().unwrap();
    let got = image(&core, &meme).unwrap();
    assert_eq!(got["media_type"], "image/png");
    let data = got["data"].as_str().unwrap();
    assert_eq!(decoded_len(data), size, "三块接起来是整张");
    assert_eq!(data.matches('=').count(), 1, "只有最后一块补位");
    let offsets: Vec<u64> = core
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|(_, params)| params["offset"].as_u64().unwrap())
        .collect();
    assert_eq!(offsets, [0, CHUNK, CHUNK * 2]);
}

#[test]
fn an_upload_is_put_into_the_core_and_refused_when_not_an_image() {
    let put = json!({"blob": "sha256:b", "media_type": "image/gif", "width": 4, "height": 5, "kind": "image"});
    let core = fake(0, put);
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
    let text = fake(0, json!({"blob": "sha256:c", "kind": "text"}));
    assert_eq!(
        store(&text, &params, 3).unwrap_err()["message"],
        "not_image"
    );
}
