//! 每一轮掷一次（设计 34 第二节第 7 条）：核心回合开始问 `user.append {session, venue, text?}`（核心施工 X-16 补），中了交一句，
//! 排在她要回的那句话后面。开关、概率照配置；库里一张能发的都没有的不提醒。原文是给模型看的，登记在 `docs/prompt-ledger.md`。
//!
//! 场所会话（群、私聊，`venue` 不空）中了以后先经 im 的 `chat.venue` 看这个场所开着的插件（设计 34 第二节第 9 条，项目主人 10-24：
//! 表情包按群开关，关着的群里不提醒），`plugins` 里没有表情包的不提醒；问不到 im 的（没装、没答）照旧提醒。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::calls::Core;
use crate::settings::Settings;

/// 这个包的编号：im 的插件开关照它。
const PACKAGE: &str = "miyu-memes";

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

/// 照现在的配置、库里有没有能发的、掷出的 `rolled` 答。
pub fn answer(settings: &Settings, stocked: bool, rolled: u8) -> Value {
    let hit = stocked && settings.reminder && rolled < settings.probability;
    if hit {
        json!({"content": [{"kind": "meme", "text": HINT}]})
    } else {
        json!({"content": []})
    }
}

/// `user.append` 的参数 `params` 是场所会话、im 说这个场所关着表情包：交回真。本机会话、问不到 im 的交回假。
pub fn silenced(core: &dyn Core, params: &Value) -> bool {
    if !params["venue"].is_string() {
        return false;
    }
    let asked =
        json!({"package": "im", "method": "chat.venue", "params": {"session": params["session"]}});
    let Ok(answer) = core.call("package.call", &asked) else {
        return false;
    };
    answer["plugins"]
        .as_array()
        .is_some_and(|plugins| !plugins.iter().any(|one| one == PACKAGE))
}

#[cfg(test)]
mod tests;
