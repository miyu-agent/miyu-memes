//! 每一轮掷一次（设计 34 第二节第 7 条）：核心回合开始问 `user.append {session, venue, text?}`（核心施工 X-16 补），中了交一句，
//! 排在她要回的那句话后面。概率、场所开关照配置；库里一张能发的都没有的不提醒。原文是给模型看的，登记在 `docs/prompt-ledger.md`。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::settings::Settings;

/// 提醒的原文：只说是什么，不下命令（旧版的经验：位置比措辞要紧，排在那句话后面就够）。
pub const HINT: &str = "<meme-hint>This reply comes with a sticker.</meme-hint>";

/// 掷骰子：0 到 99。只用标准库：时钟的纳秒加一个计数器搅一搅。
pub fn roll() -> u8 {
    static COUNTER: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| u64::from(since.subsec_nanos()));
    let mut x = nanos ^ COUNTER.fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    u8::try_from(x % 100).unwrap_or(0)
}

/// 照这一轮的 `params`、现在的配置、库里有没有能发的、掷出的 `rolled` 答。
pub fn answer(params: &Value, settings: &Settings, stocked: bool, rolled: u8) -> Value {
    let venue = params["venue"].as_str().unwrap_or_default();
    let hit = stocked && settings.reminds_in(venue) && rolled < settings.probability;
    if hit {
        json!({"content": [{"kind": "meme", "text": HINT}]})
    } else {
        json!({"content": []})
    }
}

#[cfg(test)]
mod tests;
