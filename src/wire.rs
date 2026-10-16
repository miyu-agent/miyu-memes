//! 跟核心说话（核心仓库作者说明第四节）：标准输入输出上一行一条紧凑的 JSON-RPC。读的一头在 [`serve`](crate::serve) 的主循环里，
//! 写的一头所有线程共用；我们发给核心的请求照 `id` 等回应。

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};

/// 一次请求最多等多久：`model.embed` 冷启动的本机模型要核对文件、拉起小程序。
const WAIT: Duration = Duration::from_secs(90);

/// 回应：`result`，或者 JSON-RPC 的 `error` 原样。
pub type Answer = Result<Value, Value>;

/// 和核心的这一条连接。
pub struct Wire {
    out: Mutex<Box<dyn Write + Send>>,
    waiting: Mutex<HashMap<String, Sender<Answer>>>,
    next: AtomicU64,
}

impl Wire {
    /// 往 `out` 写。
    pub fn new(out: Box<dyn Write + Send>) -> Wire {
        Wire {
            out: Mutex::new(out),
            waiting: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        }
    }

    /// 写一行。写不出去（核心关了）的不管：读的一头会读到头，程序照常退出。
    pub fn send(&self, line: &Value) {
        let mut out = self.out.lock().unwrap_or_else(PoisonError::into_inner);
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            // 核心那一头关了。
        }
    }

    /// 发一条请求，等它的回应。等不到的当回了错。
    pub fn call(&self, method: &str, params: &Value) -> Answer {
        let id = format!("m{}", self.next.fetch_add(1, Ordering::Relaxed));
        let (tell, answer) = mpsc::channel();
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.clone(), tell);
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let got = answer
            .recv_timeout(WAIT)
            .unwrap_or_else(|_| Err(json!({"message": "no answer from the core"})));
        self.waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        got
    }

    /// 读到一条回应：交给等它的那一头，对不上的不理。
    pub fn deliver(&self, id: &str, answer: Answer) {
        let tell = self
            .waiting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        if let Some(tell) = tell {
            drop(tell.send(answer));
        }
    }

    /// 回核心发来的请求 `id`。
    pub fn answer(&self, id: &Value, answer: Answer) {
        let line = match answer {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        };
        self.send(&line);
    }
}

/// 不认识的方法（JSON-RPC 的 `-32601`）。
pub fn unknown_method() -> Value {
    json!({"code": -32601, "message": "unknown method"})
}

#[cfg(test)]
mod tests;
