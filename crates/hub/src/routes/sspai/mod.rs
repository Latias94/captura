use captura_common::{Error, Result};
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;
use url::form_urlencoded;

// 路由拆分到独立文件，避免 mod.rs 过于臃肿。
pub mod activity;
pub mod author;
pub mod bookmarks;
pub mod column;
pub mod index;
pub mod matrix;
pub mod series;
pub mod series_update;
pub mod shortcuts;
pub mod tag;
pub mod topic;
pub mod topics;

/// 通用时间戳转换：将秒级 Unix 时间转换为带时区的 DateTime。
pub fn parse_unix_to_fixed(ts: i64) -> Option<DateTime<FixedOffset>> {
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(0)?;
    Some(offset.from_utc_datetime(&naive))
}

/// 少数派返回的作者信息（在多个路由中复用）。
#[derive(Debug, Deserialize)]
pub struct TagAuthor {
    pub nickname: String,
}

/// 文章详情接口的响应封装。
#[derive(Debug, Deserialize)]
pub struct ArticleDetailResp {
    pub data: ArticleDetailData,
}

/// 文章详情数据，包含正文和可选的封面图。
#[derive(Debug, Deserialize)]
pub struct ArticleDetailData {
    pub body: String,
    #[serde(default)]
    pub promote_image: Option<String>,
}

/// 通用的文章详情抓取函数，供各子路由调用。
pub async fn fetch_detail(url: &str, referer: &str) -> Result<ArticleDetailData> {
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;
    let resp = client
        .get(url)
        .header("Referer", referer)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{url} -> http status {status}")));
    }
    let detail: ArticleDetailResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai detail json parse: {e}")))?;
    Ok(detail.data)
}

/// URL 组件编码工具（与 JS encodeURIComponent 语义一致，用于 tag 等路由）。
pub fn encode_component(input: &str) -> String {
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("k", input)
        .finish();
    encoded
        .split_once('=')
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}
