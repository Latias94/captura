pub mod forum;

/// 构造 NGA 帖子摘要描述。
pub fn format_thread_description(author: &str, replies: i64) -> String {
    let mut desc = String::new();
    desc.push_str(&format!("作者：{}，回复数：{}", author, replies));
    desc.push_str("<br><br>点击链接在 NGA 查看完整内容。");
    desc
}
