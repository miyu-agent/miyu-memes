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
    assert_eq!(tools[1]["access"], "outbound", "发出去的照实写");
}
