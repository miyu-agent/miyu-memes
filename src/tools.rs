//! 登记给核心的五件工具（设计 34 第四节）：说明、参数给模型看，写英文短句、一件最多三句；调用以后才用得上的写进结果里。
//! 字由这个包自己登记（`docs/prompt-ledger.md`），改了要重量 token。

use serde_json::{Value, json};

/// 一件工具。
struct Spec {
    name: &'static str,
    description: &'static str,
    schema: Value,
    access: &'static str,
    display: [&'static str; 3],
    /// 时间线上名字后面跟哪个参数（核心 S-6 上的 `subject`）：「挑选表情包 · nvidia 驱动 崩溃」。
    subject: &'static str,
    /// AI 在命令里写脚本时能用 `miyu call` 调（核心 M-2 的 `clicall`）：只读、搜索类、和会话本身无关的开（项目主人 10-20）。
    clicall: bool,
}

/// 五件，照这个先后登记。
fn specs() -> Vec<Spec> {
    vec![
        Spec {
            name: "meme_search",
            description: "Find stickers by meaning. Returns each sticker's id, what it shows and when to use it. Skip this when you already know the id.",
            schema: json!({"type": "object", "properties": {
                "query": {"type": "string", "description": "What you want to express."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 20, "description": "Default 5."}
            }, "required": ["query"]}),
            access: "read",
            display: ["挑选表情包", "Pick a sticker", "スタンプを探す"],
            subject: "query",
            clicall: true,
        },
        Spec {
            name: "meme_send",
            description: "Send a sticker as a message of its own. It ends your turn by default, like a sticker as your last reply; set end to false to keep talking after it.",
            schema: json!({"type": "object", "properties": {
                "id": {"type": "string"},
                "end": {"type": "boolean", "description": "Default true."}
            }, "required": ["id"]}),
            // 跟她回的字一样是这一轮的回话，不往 Miyu 外面发：QQ 里由桥照回话投递。
            access: "read",
            display: ["发送表情包", "Send a sticker", "スタンプを送る"],
            subject: "id",
            clicall: false,
        },
        Spec {
            name: "meme_save",
            description: "Save an image as a sticker: a file on this computer, or an image in a chat message.",
            schema: json!({"type": "object", "properties": {
                "path": {"type": "string"},
                "msg": {"type": "string", "description": "The msg= id."},
                "index": {"type": "integer", "description": "The #n item. Default 1."},
                "reason": {"type": "string", "description": "Why you keep it."}
            }, "required": ["reason"]}),
            access: "write",
            display: ["保存表情包", "Save a sticker", "スタンプを保存"],
            subject: "reason",
            clicall: false,
        },
        Spec {
            name: "meme_edit",
            description: "Change a sticker's description or usage.",
            schema: json!({"type": "object", "properties": {
                "id": {"type": "string"},
                "description": {"type": "string"},
                "usage": {"type": "string"}
            }, "required": ["id"]}),
            access: "write",
            display: ["编辑表情包", "Edit a sticker", "スタンプを編集"],
            subject: "id",
            clicall: false,
        },
        Spec {
            name: "meme_delete",
            description: "Stop using a sticker. People can bring it back from the admin page.",
            schema: json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]}),
            access: "write",
            display: ["删除表情包", "Delete a sticker", "スタンプを削除"],
            subject: "id",
            clicall: false,
        },
    ]
}

/// 场所插件的名字（核心 O-60，设计 34 第二节第 9 条）：im 在哪个群、私聊关着表情包，那里的工具表就没有这五件；本机会话不看它。
const PLUGIN: &str = "miyu-memes";

/// `provide` 的参数：五件都给三种场所，带 `plugin`。
pub fn provide() -> Value {
    let tools: Vec<Value> = specs()
        .into_iter()
        .map(|spec| {
            let mut tool = json!({
                "name": spec.name,
                "description": spec.description,
                "input_schema": spec.schema,
                "access": spec.access,
                "venues": ["local", "private", "group"],
                "display_name": {"zh": spec.display[0], "en": spec.display[1], "ja": spec.display[2]},
                "subject": spec.subject,
                "plugin": PLUGIN,
            });
            // 只有开的才写：不写就是不开。
            if spec.clicall {
                tool["clicall"] = Value::Bool(true);
            }
            tool
        })
        .collect();
    json!({ "tools": tools })
}

#[cfg(test)]
mod tests;
