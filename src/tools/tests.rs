use super::provide;

#[test]
fn five_tools_named_with_the_package_prefix_and_short_descriptions() {
    let provided = provide();
    let tools = provided["tools"].as_array().expect("是数组");
    // 场所插件（设计 34 第二节第 9 条）：im 关着表情包的群里没有它们。
    assert!(tools.iter().all(|tool| tool["plugin"] == "memes"));
    let names: Vec<&str> = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "meme_search",
            "meme_send",
            "meme_save",
            "meme_edit",
            "meme_delete"
        ]
    );
    for tool in tools {
        let description = tool["description"].as_str().unwrap();
        let sentences = description.matches(". ").count() + 1;
        assert!(sentences <= 3, "一件最多三句：{description}");
        assert!(description.is_ascii(), "给模型看的写英文：{description}");
        assert!(tool["display_name"]["zh"].is_string());
    }
    assert_eq!(
        tools[1]["access"], "read",
        "发表情包是回话的一部分，写 outbound 每发一张都要人确认"
    );
}

#[test]
fn each_tool_names_the_argument_shown_after_its_name() {
    // 2026-10-18 项目主人：「挑选表情包」后面要写搜了什么（「挑选表情包 · nvidia 驱动 崩溃」）。核心 S-6 上：`subject`
    // 要是参数格式里有的那一个，不然整件登记不上。
    let provided = provide();
    for tool in provided["tools"].as_array().unwrap() {
        let subject = tool["subject"].as_str().expect("每件都写");
        assert!(
            tool["input_schema"]["properties"].get(subject).is_some(),
            "{subject} 不在 {} 的参数里",
            tool["name"]
        );
    }
    assert_eq!(provided["tools"][0]["subject"], "query");
}

#[test]
fn only_search_is_callable_from_scripts() {
    // 2026-10-20 项目主人：只读的、搜索类的、和会话本身无关的开给脚本（核心 M-2 的 `clicall`）；发、存、改、删都不开。
    let provided = provide();
    let callable: Vec<&str> = provided["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|tool| tool.get("clicall") == Some(&serde_json::Value::Bool(true)))
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(callable, ["meme_search"]);
    assert!(provided["tools"][1].get("clicall").is_none(), "不开的不写");
}

#[test]
fn audit_128_meme_save_declares_its_path_argument_for_the_core_to_check() {
    // 审计 #128：`meme_save` 收一个宿主机路径，三种场所都开。核心只对在 `provide` 里声明了 `paths` 的扩展工具按工作区
    // 判路径（核心 XG-1，`miyu-endpoint/src/provide/declared.rs`）；不声明，群会话、陌生人私聊（confined）也照样放行，
    // 家目录里的图能被存下再 `meme_send` 到群里。照 miyu-ocr 的 `ocr` 工具写法（`.paths([{"parameter": "path"}])`）。
    // 这是结构性断言：端到端要起核心 + im + 群会话，在这个仓里测不了。
    let provided = provide();
    let save = provided["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "meme_save")
        .expect("有 meme_save");
    let paths = save["paths"]
        .as_array()
        .unwrap_or_else(|| panic!("meme_save 没声明 paths，核心不会按工作区查 path：{save}"));
    assert!(
        paths.iter().any(|entry| entry["parameter"] == "path"),
        "meme_save 的 paths 要指向 path 参数：{paths:?}"
    );
}
