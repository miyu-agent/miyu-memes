use serde_json::json;

use super::{HINT, answer, roll};
use crate::settings::Settings;

#[test]
fn a_hit_under_the_chance_with_the_switch_on_and_stock() {
    let settings = Settings {
        probability: 10,
        ..Settings::default()
    };
    assert_eq!(answer(&settings, true, 9)["content"][0]["text"], HINT);
    assert_eq!(answer(&settings, true, 10)["content"], json!([]), "没中");
    assert_eq!(
        answer(&settings, false, 0)["content"],
        json!([]),
        "库里没有能发的"
    );
    let off = Settings {
        reminder: false,
        ..settings.clone()
    };
    assert_eq!(answer(&off, true, 0)["content"], json!([]), "关了");
    let never = Settings {
        probability: 0,
        ..settings
    };
    assert_eq!(
        answer(&never, true, 0)["content"],
        json!([]),
        "概率 0 从不提醒"
    );
}

#[test]
fn rolls_stay_in_range_and_vary() {
    let rolls: Vec<u8> = (0..200).map(|_| roll()).collect();
    assert!(rolls.iter().all(|r| *r < 100));
    assert!(rolls.iter().any(|r| *r != rolls[0]), "不是同一个数");
}

/// 假的核心：`package.call` 照 `answer` 答，记下问了什么。
struct Asked {
    answer: crate::wire::Answer,
    seen: std::sync::Mutex<Vec<serde_json::Value>>,
}

impl crate::calls::Core for Asked {
    fn call(&self, method: &str, params: &serde_json::Value) -> crate::wire::Answer {
        assert_eq!(method, "package.call");
        self.seen.lock().expect("没 panic").push(params.clone());
        self.answer.clone()
    }
}

fn asked(answer: crate::wire::Answer) -> Asked {
    Asked {
        answer,
        seen: std::sync::Mutex::new(Vec::new()),
    }
}

#[test]
fn a_venue_where_im_says_memes_is_off_gets_no_hint() {
    let group = json!({"session": "s1", "venue": "qq:group:31"});
    let off = asked(Ok(
        json!({"venue": "qq:group:31", "plugins": ["miyu-im-identity"]}),
    ));
    assert!(super::silenced(&off, &group), "关着的群不提醒");
    assert_eq!(
        off.seen.lock().expect("没 panic")[0],
        json!({"package": "miyu-im", "method": "chat.venue", "params": {"session": "s1"}})
    );
    let on = asked(Ok(json!({"plugins": ["miyu-memes"]})));
    assert!(!super::silenced(&on, &group), "开着的照旧");
    let old = asked(Ok(json!({"venue": "qq:group:31"})));
    assert!(!super::silenced(&old, &group), "旧的 im 不交 plugins：照旧");
    let gone = asked(Err(json!({"message": "unknown package"})));
    assert!(!super::silenced(&gone, &group), "问不到 im：照旧");
    let local = asked(Ok(json!({"plugins": []})));
    assert!(
        !super::silenced(&local, &json!({"session": "s2", "venue": null})),
        "本机会话不问"
    );
    assert!(local.seen.lock().expect("没 panic").is_empty());
}
