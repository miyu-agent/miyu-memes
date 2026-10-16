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
