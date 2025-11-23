use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

const API_SUBJECT_URL: &str = "https://ngabbs.com/app_api.php?__lib=subject&__act=list";
const X_UA: &str = "NGA_skull/6.0.5(iPhone10,3;iOS 12.0.1)";

pub const META_NGA_FORUM: RouteMeta = RouteMeta {
    hub_id: "nga/forum",
    path: "/nga/forum/:fid/:recommend?",
    categories: &["bbs"],
    example: "/nga/forum/489",
    params: &[
        ParamMeta {
            name: "fid",
            description: "Forum id, from NGA forum URLs.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "recommend",
            description: "If present, only show recommended threads.",
            default: None,
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["bbs.nga.cn", "nga.178.com"],
        target: "/forum/:fid",
    }],
    name: "NGA 分区帖子列表",
    maintainers: &["captura"],
    url: "https://nga.178.com",
    description: "NGA forum thread list via official mobile JSON API (subject/list), simplified list view inspired by RSSHub /nga/forum route.",
    default_view: Some("articles"),
};

#[derive(Debug, Deserialize)]
struct NgaSubjectResponse {
    code: i32,
    msg: String,
    result: NgaSubjectResult,
}

#[derive(Debug, Deserialize)]
struct NgaSubjectResult {
    #[serde(default)]
    forumname: String,
    #[serde(default)]
    data: Vec<NgaThread>,
}

#[derive(Debug, Deserialize)]
struct NgaThread {
    tid: i64,
    fid: i64,
    #[serde(default)]
    author: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    postdate: i64,
    #[serde(default)]
    replies: i64,
}

async fn fetch_forum_threads(fid: &str, recommend: bool) -> Result<NgaSubjectResult> {
    let client =
        captura_net::client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;
    let ts = chrono::Utc::now().timestamp();
    let cookie = format!("guestJs={}", ts);

    let resp = client
        .post(API_SUBJECT_URL)
        .header("X-User-Agent", X_UA)
        .header("Cookie", cookie)
        .form(&[
            ("fid", fid),
            ("recommend", if recommend { "1" } else { "0" }),
        ])
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("nga/forum: http status {}", status)));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let parsed: NgaSubjectResponse =
        serde_json::from_str(&body).map_err(|e| Error::Parse(format!("nga/forum: {e}")))?;
    if parsed.code != 0 {
        return Err(Error::Network(format!(
            "nga/forum: api error {} {}",
            parsed.code, parsed.msg
        )));
    }
    Ok(parsed.result)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let fid = ctx.param_str("fid").unwrap_or("").trim().to_string();
    if fid.is_empty() {
        return Err(captura_common::Error::Parse("fid is required".to_string()));
    }
    let recommend = ctx
        .param_str("recommend")
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let limit = ctx.param_i64("limit").unwrap_or(35).max(1) as usize;

    let res = fetch_forum_threads(&fid, recommend).await?;
    let forum_name = if res.forumname.is_empty() {
        format!("NGA forum {}", fid)
    } else {
        format!("NGA - {}", res.forumname)
    };

    let mut items = Vec::new();
    for t in res.data.into_iter().filter(|t| t.tid != 0).take(limit) {
        let title = if t.subject.trim().is_empty() {
            format!("主题 #{}", t.tid)
        } else {
            t.subject.clone()
        };
        let link = format!("https://nga.178.com/read.php?tid={}", t.tid);
        let pub_date = util::parse_unix_timestamp(t.postdate, 0);
        let desc = crate::routes::nga::format_thread_description(&t.author, t.replies);

        items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(link),
            author: if t.author.trim().is_empty() {
                None
            } else {
                Some(t.author.clone())
            },
            pub_date,
            categories: Vec::new(),
        });
    }

    let link = format!("https://nga.178.com/thread.php?fid={}", fid);
    let mut title = forum_name;
    if recommend {
        title.push_str(" - 精华");
    }

    Ok(HubData {
        title,
        description: Some(
            "NGA 论坛分区帖子列表，使用移动端 JSON 接口（仅列表信息，不含全文）".to_string(),
        ),
        link: Some(link),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NGA_FORUM: Route = Route {
    meta: &META_NGA_FORUM,
    handler: handler_fn,
};
