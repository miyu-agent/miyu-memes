use serde_json::json;

use super::{Library, NewMeme};

fn meme(hex: &str) -> NewMeme {
    NewMeme {
        hash: format!("sha256:{hex}"),
        media_type: "image/png".to_string(),
        width: 120,
        height: 96,
        description: "A cat waving.".to_string(),
        usage: "Say hi.".to_string(),
        source: json!({"kind": "ai"}),
        reason: None,
    }
}

#[test]
fn the_id_is_the_hash_prefix_and_the_same_image_is_stored_once() {
    let library = Library::in_memory().unwrap();
    let a = format!("abcdef01{}", "0".repeat(56));
    let (id, new) = library.add(&meme(&a), 1).unwrap();
    assert_eq!((id.as_str(), new), ("abcdef01", true));
    assert_eq!(
        library.add(&meme(&a), 2).unwrap(),
        ("abcdef01".to_string(), false)
    );
    let b = format!("abcdef01{}", "1".repeat(56));
    assert_eq!(
        library.add(&meme(&b), 3).unwrap().0,
        "abcdef011",
        "撞了往后多取"
    );
}

#[test]
fn editing_disabling_and_purging() {
    let library = Library::in_memory().unwrap();
    let (id, _) = library.add(&meme(&"a".repeat(64)), 1).unwrap();
    library.put_vector(&id, "local:tiny", &[0.6, 0.8]).unwrap();
    assert!(library.edit(&id, Some("A cat."), None).unwrap());
    let got = library.get(&id).unwrap().unwrap();
    assert_eq!(
        (got.description.as_str(), got.usage.as_str()),
        ("A cat.", "Say hi.")
    );
    assert!(
        library.vectors("local:tiny").unwrap().is_empty(),
        "改了描述，向量要重算"
    );
    assert!(library.set_enabled(&id, false).unwrap());
    assert!(library.all(true).unwrap().is_empty());
    assert_eq!(library.all(false).unwrap().len(), 1, "停用的后台页看得到");
    assert!(library.purge(&id).unwrap());
    assert!(library.get(&id).unwrap().is_none());
    assert!(!library.edit("nope", Some("x"), None).unwrap());
}

#[test]
fn vectors_round_trip_per_model() {
    let library = Library::in_memory().unwrap();
    let (id, _) = library.add(&meme(&"b".repeat(64)), 1).unwrap();
    library.put_vector(&id, "local:tiny", &[0.6, 0.8]).unwrap();
    assert_eq!(
        library.vectors("local:tiny").unwrap(),
        vec![(id, vec![0.6, 0.8])]
    );
    assert!(
        library.vectors("emb/other").unwrap().is_empty(),
        "换了模型的不算"
    );
}

#[test]
fn an_old_library_gets_the_keywords_column_and_changed_keywords_drop_vectors() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE memes (id TEXT PRIMARY KEY, hash TEXT NOT NULL UNIQUE, media_type TEXT NOT NULL,
         width INTEGER NOT NULL, height INTEGER NOT NULL, description TEXT NOT NULL, usage TEXT NOT NULL,
         source TEXT NOT NULL, reason TEXT, enabled INTEGER NOT NULL DEFAULT 1, uses INTEGER NOT NULL DEFAULT 0,
         last_used INTEGER, created INTEGER NOT NULL);",
    )
    .unwrap();
    super::prepare(&db).unwrap();
    super::prepare(&db).unwrap();
    let library = Library { db, thumbs: None };
    let (id, _) = library.add(&meme("ab"), 1).unwrap();
    assert_eq!(library.get(&id).unwrap().unwrap().keywords, "");
    library.put_vector(&id, "m", &[1.0]).unwrap();
    assert!(library.set_keywords(&id, "KFC").unwrap());
    assert_eq!(library.get(&id).unwrap().unwrap().keywords, "KFC");
    assert!(
        library.vectors("m").unwrap().is_empty(),
        "检索词变了，旧向量作废"
    );
    library.put_vector(&id, "m", &[1.0]).unwrap();
    assert!(!library.set_keywords(&id, "KFC").unwrap(), "没变的不动");
    assert_eq!(library.vectors("m").unwrap().len(), 1);
}
