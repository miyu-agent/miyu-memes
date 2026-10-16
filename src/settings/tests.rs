use serde_json::json;

use super::Settings;

#[test]
fn keys_of_this_package_are_applied_and_null_restores_the_default() {
    let mut settings = Settings::default();
    let keys = json!({
        "miyu-memes.reminder_probability": 40,
        "miyu-memes.reminder_group": false,
        "miyu-memes.max_size_mb": 2,
        "other.reminder_probability": 99
    });
    settings.apply(keys.as_object().unwrap());
    assert_eq!(settings.probability, 40);
    assert!(!settings.reminds_in("group"));
    assert!(settings.reminds_in("local"));
    assert_eq!(settings.max_bytes, 2 * 1024 * 1024);
    settings.apply(
        json!({"miyu-memes.reminder_probability": null})
            .as_object()
            .unwrap(),
    );
    assert_eq!(settings.probability, 3, "变成没设的回出厂值");
    assert!(!settings.reminds_in("elsewhere"));
}
