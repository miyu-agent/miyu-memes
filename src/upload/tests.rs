//! 交给核心存成 blob（施工 M-12）：小的一条 `blob.put`；大的分块传，每一行都在核心收的 1 MiB 以内，拼回来和原来一样。

use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::{CHUNK, PUT_MOST, upload};
use crate::calls::Core;
use crate::wire::Answer;

/// 核心收一行最多多少字节。
const LINE_LIMIT: usize = 1024 * 1024;

/// 假核心：照真核心的规矩，一行（方法加参数写成 JSON）超了 1 MiB 的拒；记下收到的字节和问过的方法。
#[derive(Default)]
struct Fake {
    got: Mutex<Vec<u8>>,
    methods: Mutex<Vec<String>>,
    /// `blob.write` 回的 `received` 不往前走（核心卡住）。
    stuck: bool,
}

impl Core for Fake {
    fn call(&self, method: &str, params: &Value) -> Answer {
        let line = json!({"method": method, "params": params}).to_string();
        if line.len() > LINE_LIMIT {
            return Err(json!({"message": "line too long"}));
        }
        self.methods.lock().unwrap().push(method.to_string());
        let mut got = self.got.lock().unwrap();
        match method {
            "blob.put" => {
                got.extend(STANDARD.decode(params["data"].as_str().unwrap()).unwrap());
                Ok(json!({"blob": "sha256:put", "kind": "image"}))
            }
            "blob.open" => Ok(json!({"upload": "u-1"})),
            "blob.write" => {
                assert_eq!(
                    usize::try_from(params["offset"].as_u64().unwrap()).unwrap(),
                    got.len(),
                    "照收到的接着写"
                );
                if !self.stuck {
                    got.extend(STANDARD.decode(params["data"].as_str().unwrap()).unwrap());
                }
                Ok(json!({"received": got.len()}))
            }
            "blob.close" => {
                Ok(json!({"blob": "sha256:chunked", "kind": "image", "width": 2, "height": 3}))
            }
            _ => Err(json!({"code": -32601})),
        }
    }
}

/// 一份假图：`len` 个字节，照位置变，拼错了看得出来。
fn picture(len: usize) -> Vec<u8> {
    (0..len).map(|at| u8::try_from(at % 251).unwrap()).collect()
}

#[test]
fn a_small_picture_goes_in_one_put() {
    let core = Fake::default();
    let bytes = picture(PUT_MOST);
    let put = upload(&core, "a.gif", &bytes).unwrap();
    assert_eq!(put["blob"], "sha256:put");
    assert_eq!(*core.methods.lock().unwrap(), ["blob.put"]);
    assert_eq!(*core.got.lock().unwrap(), bytes);
}

#[test]
fn a_big_picture_goes_in_chunks_each_under_the_line_limit() {
    let core = Fake::default();
    let bytes = picture(3 * 1024 * 1024 + 5);
    let put = upload(&core, "big.gif", &bytes).unwrap();
    assert_eq!(put["blob"], "sha256:chunked", "回应同 blob.put：{put}");
    assert_eq!(*core.got.lock().unwrap(), bytes, "拼回来一字不差");
    let methods = core.methods.lock().unwrap();
    assert_eq!(methods.first().map(String::as_str), Some("blob.open"));
    assert_eq!(methods.last().map(String::as_str), Some("blob.close"));
    assert_eq!(
        methods.iter().filter(|one| *one == "blob.write").count(),
        bytes.len().div_ceil(CHUNK)
    );
}

#[test]
fn a_stuck_upload_fails_instead_of_looping() {
    let core = Fake {
        stuck: true,
        ..Fake::default()
    };
    assert!(upload(&core, "big.gif", &picture(PUT_MOST + 1)).is_err());
}
