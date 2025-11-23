use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const HOST_CN: &str = "https://leetcode.cn";
const GQL_ENDPOINT_CN: &str = "https://leetcode.cn/graphql";

#[derive(Debug, Deserialize)]
struct DailyRecord {
    date: String,
    question: DailyQuestionShort,
}

#[derive(Debug, Deserialize)]
struct DailyQuestionShort {
    #[serde(rename = "questionFrontendId")]
    frontend_id: String,
    titleSlug: String,
}

#[derive(Debug, Deserialize)]
struct DailyQuestionData {
    todayRecord: Vec<DailyRecord>,
}

#[derive(Debug, Deserialize)]
struct DailyQuestionResponse {
    data: DailyQuestionData,
}

#[derive(Debug, Deserialize)]
struct QuestionTag {
    slug: String,
    #[serde(default)]
    translatedName: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QuestionDetail {
    #[serde(rename = "questionFrontendId")]
    frontend_id: String,
    title: String,
    #[serde(default)]
    translatedTitle: Option<String>,
    #[serde(default)]
    translatedContent: Option<String>,
    difficulty: String,
    topicTags: Vec<QuestionTag>,
}

#[derive(Debug, Deserialize)]
struct QuestionDetailData {
    question: QuestionDetail,
}

#[derive(Debug, Deserialize)]
struct QuestionDetailResponse {
    data: QuestionDetailData,
}

pub const META_LEETCODE_DAILY_CN: RouteMeta = RouteMeta {
    hub_id: "leetcode/daily-cn",
    path: "/leetcode/daily-cn",
    categories: &["programming"],
    example: "/leetcode/daily-cn",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["leetcode.cn"],
        target: "/dailyquestion/cn",
    }],
    name: "LeetCode 每日一题（中文）",
    maintainers: &["captura"],
    url: "https://leetcode.cn/",
    description: "LeetCode.cn 每日一题（中文），对标 RSSHub /leetcode/dailyquestion-cn 路由的精简实现。",
    default_view: Some("articles"),
};

fn parse_date_utc(date: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(date)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("leetcode client error: {}", e)))?;

    // 1) 获取今日一题基本信息（日期 + titleSlug）
    let daily_payload = serde_json::json!({
        "query": "query questionOfToday { todayRecord { date question { questionFrontendId titleSlug } } }",
        "variables": {},
    });
    let daily_resp = client
        .post(GQL_ENDPOINT_CN)
        .header("content-type", "application/json")
        .json(&daily_payload)
        .send()
        .await
        .map_err(|e| Error::Network(format!("leetcode dailyquestion-cn: {}", e)))?;
    if !daily_resp.status().is_success() {
        return Err(Error::Network(format!(
            "leetcode dailyquestion-cn: http status {}",
            daily_resp.status()
        )));
    }
    let daily: DailyQuestionResponse = daily_resp
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;
    let record = daily
        .data
        .todayRecord
        .into_iter()
        .next()
        .ok_or_else(|| Error::Parse("leetcode: empty todayRecord".into()))?;

    let date_str = record.date;
    let slug = record.question.titleSlug;
    let frontend_id = record.question.frontend_id;
    let link = format!("{}/problems/{}/", HOST_CN, slug);

    // 2) 获取题目详情（中文内容 + 难度 + 标签）
    let detail_payload = serde_json::json!({
        "operationName": "questionData",
        "variables": { "titleSlug": slug },
        "query": "query questionData($titleSlug: String!) { question(titleSlug: $titleSlug) { questionFrontendId title titleSlug translatedTitle translatedContent difficulty topicTags { slug translatedName } } }",
    });

    let detail_resp = client
        .post(GQL_ENDPOINT_CN)
        .header("content-type", "application/json")
        .json(&detail_payload)
        .send()
        .await
        .map_err(|e| Error::Network(format!("leetcode questionData: {}", e)))?;
    if !detail_resp.status().is_success() {
        return Err(Error::Network(format!(
            "leetcode questionData: http status {}",
            detail_resp.status()
        )));
    }
    let detail: QuestionDetailResponse = detail_resp
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;
    let q = detail.data.question;

    let emoji = match q.difficulty.as_str() {
        "Medium" => "🟡",
        "Hard" => "🔴",
        _ => "🟢",
    };

    let title_base = q.translatedTitle.clone().unwrap_or_else(|| q.title.clone());
    let item_title = format!("{} {}. {}", emoji, frontend_id, title_base);

    let mut tags_str = String::new();
    if !q.topicTags.is_empty() {
        let tags: Vec<String> = q
            .topicTags
            .iter()
            .map(|t| {
                let mut s = String::from("#");
                s.push_str(&t.slug.replace('-', "_"));
                s
            })
            .collect();
        tags_str = tags.join(" ");
    }

    let mut desc = String::new();
    if let Some(ref html) = q.translatedContent {
        desc.push_str(html);
    }
    if !tags_str.is_empty() {
        desc.push_str("<p>");
        desc.push_str(&html_escape::encode_safe(&tags_str));
        desc.push_str("</p>");
    }

    let pub_date = parse_date_utc(&format!("{} 00:00:00", date_str));

    let item = HubItem {
        title: item_title,
        description: Some(desc),
        link: Some(link.clone()),
        author: None,
        pub_date,
        categories: vec!["leetcode".to_string(), "daily-cn".to_string()],
    };

    Ok(HubData {
        title: "LeetCode 每日一题（中文）".to_string(),
        description: Some("LeetCode.cn 每日一题。".to_string()),
        link: Some(HOST_CN.to_string()),
        image: None,
        language: Some("zh-CN".to_string()),
        items: vec![item],
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_LEETCODE_DAILY_CN: Route = Route {
    meta: &META_LEETCODE_DAILY_CN,
    handler: handler_fn,
};
