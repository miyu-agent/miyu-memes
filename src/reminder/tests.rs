use serde_json::json;

use super::{HINT, answer, roll};
use crate::settings::Settings;

#[test]
fn a_hit_under_the_chance_in_an_open_venue_with_stock() {
    let settings = Settings {
        probability: 10,
        ..Settings::default()
    };
    let local = json!({"session": "s", "venue": "local"});
    assert_eq!(
        answer(&local, &settings, true, 9)["content"][0]["text"],
        HINT
    );
    assert_eq!(
        answer(&local, &settings, true, 10)["content"],
        json!([]),
        "没中"
    );
    assert_eq!(
        answer(&local, &settings, false, 0)["content"],
        json!([]),
        "库里没有能发的"
    );
    let closed = Settings {
        group: false,
        ..settings.clone()
    };
    assert_eq!(
        answer(&json!({"venue": "group"}), &closed, true, 0)["content"],
        json!([]),
        "群里关了"
    );
    let never = Settings {
        probability: 0,
        ..settings
    };
    assert_eq!(
        answer(&local, &never, true, 0)["content"],
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
