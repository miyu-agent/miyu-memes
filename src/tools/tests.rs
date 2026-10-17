use super::provide;

#[test]
fn five_tools_named_with_the_package_prefix_and_short_descriptions() {
    let provided = provide();
    let tools = provided["tools"].as_array().expect("是数组");
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
