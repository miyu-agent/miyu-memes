use std::io;
use std::sync::{Arc, Mutex};

use serde_json::json;

use super::Wire;

/// 写进内存的一头，测试读回来。
#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("锁得上").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Sink {
    /// 写完了的几行：一行可能分几次写进来，没写到换行的那一截不算。
    fn lines(&self) -> Vec<serde_json::Value> {
        let text = String::from_utf8(self.0.lock().expect("锁得上").clone()).expect("是字");
        let done = text.rfind('\n').map_or("", |end| &text[..end]);
        done.lines()
            .map(|line| serde_json::from_str(line).expect("一行一条 JSON"))
            .collect()
    }
}

#[test]
fn a_call_waits_for_the_answer_with_its_id() {
    let sink = Sink::default();
    let wire = Arc::new(Wire::new(Box::new(sink.clone())));
    let asking = Arc::clone(&wire);
    let call = std::thread::spawn(move || asking.call("blob.put", &json!({"path": "/x.png"})));
    let id = loop {
        if let Some(line) = sink.lines().first() {
            break line["id"].as_str().expect("有编号").to_string();
        }
        std::thread::yield_now();
    };
    wire.deliver("someone-else", Ok(json!({"nope": true})));
    wire.deliver(&id, Ok(json!({"blob": "sha256:00"})));
    assert_eq!(call.join().expect("没崩"), Ok(json!({"blob": "sha256:00"})));
    assert_eq!(sink.lines()[0]["method"], "blob.put");
}

#[test]
fn answers_carry_the_request_id_and_errors_stay_errors() {
    let sink = Sink::default();
    let wire = Wire::new(Box::new(sink.clone()));
    wire.answer(&json!("core-1"), Ok(json!({"ok": 1})));
    wire.answer(&json!("core-2"), Err(super::unknown_method()));
    let lines = sink.lines();
    assert_eq!(
        lines[0],
        json!({"jsonrpc": "2.0", "id": "core-1", "result": {"ok": 1}})
    );
    assert_eq!(lines[1]["error"]["code"], -32601);
}
