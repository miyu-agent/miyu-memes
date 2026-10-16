//! 这个包的配置项（清单 `[settings]`，设计 34 第六节）：握手时核心交最终值，变了推 `extension.config`，只放变了的键、变成没设的
//! 是 `null`（作者说明第四节第 3 条）。键是包的编号加名字。

use serde_json::Value;

/// 包的编号：配置项的键照它开头。
const PACKAGE: &str = "miyu-memes";

/// 现在的值。
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// 自动提醒发表情包开着没有（项目主人 10-16 定：一个开关，不分群里、私聊、本机）。
    pub reminder: bool,
    /// 每一轮提醒的概率，百分数。
    pub probability: u8,
    /// 一张最大多少字节。
    pub max_bytes: u64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            reminder: true,
            probability: 3,
            max_bytes: 5 * 1024 * 1024,
        }
    }
}

impl Settings {
    /// 照 `keys` 换：认得的换上，`null` 的回出厂值，别的不理。
    pub fn apply(&mut self, keys: &serde_json::Map<String, Value>) {
        let fresh = Settings::default();
        for (key, value) in keys {
            let Some(name) = key
                .strip_prefix(PACKAGE)
                .and_then(|rest| rest.strip_prefix('.'))
            else {
                continue;
            };
            match name {
                "reminder_probability" => {
                    self.probability = value
                        .as_u64()
                        .and_then(|n| u8::try_from(n.min(100)).ok())
                        .unwrap_or(fresh.probability);
                }
                "reminder" => self.reminder = value.as_bool().unwrap_or(fresh.reminder),
                "max_size_mb" => {
                    self.max_bytes = value
                        .as_u64()
                        .map_or(fresh.max_bytes, |mb| mb.clamp(1, 20) * 1024 * 1024);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests;
