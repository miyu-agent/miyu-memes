use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};

use super::{Synced, digest, sync};
use crate::calls::{Core, State};
use crate::library::{Library, NewMeme};
use crate::settings::Settings;
use crate::wire::Answer;

/// 一块多大：小一点，测分块拼回。
const CHUNK: usize = 4;

/// 假核心：几个内容包（包 → 路径 → 字节），记下读了几次图。名字以 `.txt` 结尾的 `blob.put` 当不是图。
#[derive(Default)]
struct Fake {
    packs: Mutex<BTreeMap<String, BTreeMap<String, Vec<u8>>>>,
    image_reads: Mutex<usize>,
}

impl Core for Fake {
    fn call(&self, method: &str, params: &Value) -> Answer {
        let packs = self.packs.lock().unwrap();
        match method {
            "content.list" => {
                let listed: Vec<Value> = packs.keys().map(|id| json!({"package": id})).collect();
                Ok(json!({ "packages": listed }))
            }
            "content.read" => {
                let path = params["path"].as_str().unwrap();
                let bytes = packs
                    .get(params["package"].as_str().unwrap())
                    .and_then(|files| files.get(path))
                    .ok_or_else(|| json!({"code": -32010}))?;
                if path != "index.json" && params["offset"] == 0 {
                    *self.image_reads.lock().unwrap() += 1;
                }
                let from = usize::try_from(params["offset"].as_u64().unwrap()).unwrap();
                let to = (from + CHUNK).min(bytes.len());
                Ok(
                    json!({"data": STANDARD.encode(&bytes[from..to]), "size": bytes.len(), "eof": to == bytes.len()}),
                )
            }
            "blob.put" => {
                let data = STANDARD.decode(params["data"].as_str().unwrap()).unwrap();
                let kind = if std::path::Path::new(params["name"].as_str().unwrap())
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
                {
                    "text"
                } else {
                    "image"
                };
                Ok(
                    json!({"blob": format!("sha256:{:0>64}", digest(&data)), "media_type": "image/png",
                          "width": 2, "height": 3, "kind": kind}),
                )
            }
            _ => Err(json!({"code": -32601})),
        }
    }
}

impl Fake {
    fn pack(&self, id: &str, files: &[(&str, &[u8])]) {
        let files = files
            .iter()
            .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
            .collect();
        self.packs.lock().unwrap().insert(id.to_string(), files);
    }

    fn reads(&self) -> usize {
        *self.image_reads.lock().unwrap()
    }
}

fn state(core: &Arc<Fake>) -> State {
    State {
        library: Mutex::new(Library::in_memory().unwrap()),
        settings: Mutex::new(Settings::default()),
        core: Arc::clone(core) as Arc<dyn Core>,
    }
}

fn index(entries: &[(&str, &str)]) -> Vec<u8> {
    let memes: Vec<Value> = entries
        .iter()
        .map(|(file, usage)| json!({"file": file, "description": format!("{file} desc"), "usage": usage, "tags": ["x"]}))
        .collect();
    json!({"library": "old", "version": 2, "memes": memes})
        .to_string()
        .into_bytes()
}

fn sources(state: &State) -> Vec<(String, String)> {
    state
        .library
        .lock()
        .unwrap()
        .all(false)
        .unwrap()
        .into_iter()
        .map(|meme| (meme.description, meme.source.to_string()))
        .collect()
}

#[test]
fn a_pack_is_read_in_once_skipping_what_is_not_a_sticker() {
    let core = Arc::new(Fake::default());
    let entries = [
        ("images/a.png", "Hi."),
        ("images/b.gif", "Bye."),
        ("notes.txt", "No."),
        ("images/c.png", ""),
    ];
    core.pack(
        "shorin",
        &[
            ("index.json", &index(&entries)),
            ("images/a.png", b"aaaaaaaaaa"),
            ("images/b.gif", b"bb"),
            ("notes.txt", b"text"),
        ],
    );
    let state = state(&core);
    let first = sync(&state).unwrap();
    assert_eq!(
        first,
        Synced {
            packs: 1,
            added: 2,
            removed: 0,
            skipped: 2
        },
        "不是图的、少了用法的跳过"
    );
    let all = sources(&state);
    assert!(
        all.iter()
            .all(|(_, source)| source == r#"{"kind":"pack","package":"shorin"}"#),
        "{all:?}"
    );
    assert_eq!(core.reads(), 3);
    let again = sync(&state).unwrap();
    assert_eq!(again, Synced::default(), "index.json 没变的不再读");
    assert_eq!(core.reads(), 3, "图一张都没再读");
}

#[test]
fn dropped_entries_and_gone_packs_leave_but_edited_and_human_ones_stay() {
    let core = Arc::new(Fake::default());
    let files: [(&str, &[u8]); 3] = [("a.png", b"aa"), ("b.png", b"bbb"), ("c.png", b"cccc")];
    let mut all: Vec<(&str, &[u8])> = files.to_vec();
    let full = index(&[("a.png", "1"), ("b.png", "2"), ("c.png", "3")]);
    all.push(("index.json", &full));
    core.pack("shorin", &all);
    let state = state(&core);
    // 人先传过 c 那一张：库里照人的来源记，包带来的同一张不顶替它。
    let hash = format!("sha256:{:0>64}", digest(b"cccc"));
    let human = NewMeme {
        hash,
        media_type: "image/png".into(),
        width: 1,
        height: 1,
        description: "mine".into(),
        usage: "mine".into(),
        source: json!({"kind": "human"}),
        reason: None,
    };
    state.library.lock().unwrap().add(&human, 1).unwrap();
    assert_eq!(sync(&state).unwrap().added, 2);
    let edited_id = state
        .library
        .lock()
        .unwrap()
        .all(false)
        .unwrap()
        .into_iter()
        .find(|meme| meme.description == "b.png desc")
        .unwrap()
        .id;
    state
        .library
        .lock()
        .unwrap()
        .edit(&edited_id, Some("changed by a person"), None)
        .unwrap();

    let fewer = index(&[("c.png", "3")]);
    all[3] = ("index.json", &fewer);
    core.pack("shorin", &all);
    let pruned = sync(&state).unwrap();
    assert_eq!(
        (pruned.packs, pruned.removed),
        (1, 1),
        "a 拿掉；b 人改过留下；c 是人的"
    );
    let left: Vec<String> = sources(&state)
        .into_iter()
        .map(|(description, _)| description)
        .collect();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(
        left.contains(&"changed by a person".to_string()) && left.contains(&"mine".to_string())
    );

    core.packs.lock().unwrap().clear();
    let gone = sync(&state).unwrap();
    assert_eq!(gone.removed, 0, "剩下的都不是没改过的包里的");
    assert!(
        state.library.lock().unwrap().packs().unwrap().is_empty(),
        "不在了的包不再记"
    );
    assert_eq!(sources(&state).len(), 2);
}
