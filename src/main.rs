//! miyu-memes：Miyu 的表情包软件包（核心仓库设计 34）。核心照清单 `[process] args = ["serve"]` 拉起，标准输入输出上说协议；
//! 工作目录是核心给的这个包放状态的目录，库放在那里。

mod calls;
mod describe;
mod library;
mod methods;
mod packs;
mod reminder;
mod search;
mod settings;
mod shrink;
mod tools;
mod wire;

use std::io::{self, BufRead};
use std::path::Path;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};

use calls::State;
use library::Library;
use settings::Settings;
use wire::Wire;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("serve") {
        return serve();
    }
    eprintln!("usage: miyu-memes serve");
    ExitCode::from(2)
}

/// 跑起来：开库、握手、登记工具，然后照核心发来的办，标准输入读到头就退出。
fn serve() -> ExitCode {
    let library = match Library::open(Path::new("library.db")) {
        Ok(library) => library,
        Err(error) => {
            // 退出码 1：库打不开，重启也没用，核心不重试（作者说明第四节第 9 条）。
            eprintln!("cannot open library.db: {error}");
            return ExitCode::from(1);
        }
    };
    let wire = Arc::new(Wire::new(Box::new(io::stdout())));
    let state = Arc::new(State {
        library: Mutex::new(library),
        settings: Mutex::new(Settings::default()),
        core: Arc::clone(&wire) as Arc<dyn calls::Core>,
    });
    let reading = {
        let (wire, state) = (Arc::clone(&wire), Arc::clone(&state));
        std::thread::spawn(move || read(&wire, &state))
    };
    let hello = json!({"protocol": [1, 1], "head": {"kind": "miyu-memes", "version": env!("CARGO_PKG_VERSION")}});
    match wire.call("hello", &hello) {
        Ok(answer) => {
            if let Some(config) = answer["config"].as_object() {
                state
                    .settings
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .apply(config);
            }
        }
        Err(error) => {
            eprintln!("hello refused: {error}");
            return ExitCode::from(2);
        }
    }
    if let Err(error) = wire.call("provide", &tools::provide()) {
        eprintln!("provide refused: {error}");
        return ExitCode::from(2);
    }
    if let Err(error) = wire.call("package.methods", &methods::register()) {
        eprintln!("package.methods refused: {error}");
        return ExitCode::from(2);
    }
    resync(&state);
    drop(reading.join());
    ExitCode::SUCCESS
}

/// 读核心发来的：回应交给等它的，请求各开一个线程办，通知当场办。
fn read(wire: &Arc<Wire>, state: &Arc<State>) {
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match (
            message.get("method").and_then(Value::as_str),
            message.get("id"),
        ) {
            (Some(method), Some(id)) => {
                let (wire, state) = (Arc::clone(wire), Arc::clone(state));
                let (method, id, params) =
                    (method.to_string(), id.clone(), message["params"].clone());
                std::thread::spawn(move || wire.answer(&id, request(&state, &method, &params)));
            }
            (Some("extension.config"), None) => {
                if let Some(keys) = message["params"]["keys"].as_object() {
                    state
                        .settings
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .apply(keys);
                }
            }
            (Some("content.changed"), None) => resync(state),
            (None, Some(id)) => {
                let answer = match message.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(message["result"].clone()),
                };
                wire.deliver(id.as_str().unwrap_or_default(), answer);
            }
            (Some(_) | None, None) => {}
        }
    }
}

/// 在后台把内容包对一遍（起来时、核心推 `content.changed` 时）：读图要等核心，不挡着读的一头。
fn resync(state: &Arc<State>) {
    let state = Arc::clone(state);
    std::thread::spawn(move || match packs::sync(&state) {
        Ok(synced) => eprintln!("content packs synced: {synced:?}"),
        Err(error) => eprintln!("content packs not synced: {error}"),
    });
}

/// 办核心发来的一条请求。
fn request(state: &State, method: &str, params: &Value) -> wire::Answer {
    match method {
        "tool.call" => Ok(calls::tool(state, params)),
        "user.append" => {
            let settings = state
                .settings
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let stocked = state
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .all(true)
                .is_ok_and(|memes| !memes.is_empty());
            Ok(reminder::answer(&settings, stocked, reminder::roll()))
        }
        "method.call" => methods::call(state, params),
        _ => Err(wire::unknown_method()),
    }
}
