use std::cell::RefCell;

use serde_json::json;

use super::{Embed, reindex, search};
use crate::library::{Library, NewMeme};

/// 假的算向量：照字里有没有「猫」「狗」给二维向量；`off` 的当没配。
struct Fake {
    off: bool,
    asked: RefCell<usize>,
}

impl Embed for Fake {
    fn embed(&self, texts: &[String]) -> Option<(String, Vec<Vec<f32>>)> {
        if self.off {
            return None;
        }
        *self.asked.borrow_mut() += texts.len();
        let vectors = texts
            .iter()
            .map(|text| {
                if text.contains('猫') {
                    vec![1.0, 0.0]
                } else {
                    vec![0.0, 1.0]
                }
            })
            .collect();
        Some(("emb/fake".to_string(), vectors))
    }
}

fn library() -> Library {
    let library = Library::in_memory().unwrap();
    for (hex, description, usage) in [
        ("a", "一只猫在挥手", "打招呼"),
        ("b", "一只狗在睡觉", "困了"),
    ] {
        library
            .add(
                &NewMeme {
                    hash: format!("sha256:{}", hex.repeat(64)),
                    media_type: "image/png".to_string(),
                    width: 1,
                    height: 1,
                    description: description.to_string(),
                    usage: usage.to_string(),
                    source: json!({"kind": "ai"}),
                    reason: None,
                },
                1,
            )
            .unwrap();
    }
    library
}

#[test]
fn meaning_ranks_first_and_vectors_are_computed_once() {
    let library = library();
    let fake = Fake {
        off: false,
        asked: RefCell::new(0),
    };
    let found = search(&library, &fake, "小猫", 5).unwrap();
    assert_eq!(found[0].description, "一只猫在挥手");
    assert_eq!(*fake.asked.borrow(), 3, "问句一条、两张各补一条");
    search(&library, &fake, "小猫", 5).unwrap();
    assert_eq!(*fake.asked.borrow(), 4, "第二次只算问句");
}

#[test]
fn without_embedding_keywords_still_find_and_misses_are_empty() {
    let library = library();
    let fake = Fake {
        off: true,
        asked: RefCell::new(0),
    };
    let found = search(&library, &fake, "睡觉", 5).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].description, "一只狗在睡觉");
    assert!(search(&library, &fake, "火箭", 5).unwrap().is_empty());
}

#[test]
fn audit_131_reindex_with_embedding_off_keeps_the_vectors_it_had() {
    // 审计 #131：`reindex` 先 `clear_vectors()` 再第一次调 embed；embed 用不了（没配、断了）时交回 `Ok(None)`，
    // 原有的向量已经清光，要等下次搜索再白算一遍。应先算出第一批再清。
    let library = library();
    let on = Fake {
        off: false,
        asked: RefCell::new(0),
    };
    search(&library, &on, "小猫", 5).unwrap();
    let before = library.vectors("emb/fake").unwrap().len();
    assert_eq!(before, 2, "先让两张都有向量");
    let off = Fake {
        off: true,
        asked: RefCell::new(0),
    };
    assert!(reindex(&library, &off).unwrap().is_none(), "embed 用不了");
    assert_eq!(
        library.vectors("emb/fake").unwrap().len(),
        before,
        "embed 用不了时重建索引不该先把已有的向量清掉"
    );
}
