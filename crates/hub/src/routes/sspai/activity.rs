use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct UserInfoResp {
    data: UserInfoData,
}

#[derive(Debug, Deserialize)]
struct UserInfoData {
    nickname: String,
}

pub const META_SSPAI_ACTIVITY: RouteMeta = RouteMeta {
    hub_id: "sspai/activity",
    path: "/sspai/activity/:slug",
    categories: &["new-media"],
    example: "/sspai/activity/so1ar",
    params: &[ParamMeta {
        name: "slug",
        description: "作者 slug，可在作者主页 URL 中找到。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["sspai.com/u/:id/updates"],
        target: "/activity/:id",
    }],
    name: "SSPAI Activity",
    maintainers: &["captura"],
    url: "https://sspai.com/",
    description: "少数派作者动态更新，对标 RSSHub /sspai/activity/:slug 路由（已适配新版 activity 接口）。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let slug = ctx
        .param_str("slug")
        .ok_or_else(|| Error::Config("slug is required for sspai/activity".into()))?;

    let base_link = format!("https://sspai.com/u/{}/updates", slug);
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("sspai client error: {}", e)))?;

    // 用户信息（主要用于昵称）
    let user_url = format!("https://sspai.com/api/v1/user/slug/info/get?slug={}", slug);
    let user_resp = client
        .get(&user_url)
        .header("Referer", &base_link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{user_url} -> {e}")))?;
    let status = user_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{user_url} -> http status {status}"
        )));
    }
    let user: UserInfoResp = user_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai activity user json parse: {e}")))?;
    let nickname = user.data.nickname;

    // 动态列表
    let api_url = format!(
        "https://sspai.com/api/v1/information/user/activity/page/get?limit=10&offset=0&slug={}",
        slug
    );
    let act_resp = client
        .get(&api_url)
        .header("Referer", &base_link)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = act_resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }
    let act_json: Value = act_resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("sspai activity json parse: {e}")))?;

    let data_arr = act_json
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut items = Vec::new();

    for item in data_arr {
        let key = item
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let action = item
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let memo = item
            .get("memo")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let created_at = item.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0);

        let data = item.get("data").cloned().unwrap_or(Value::Null);

        // 尝试从 data 中提取一个「主题/文章标题」
        let mut title_core = data
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        if title_core.is_empty() {
            if let Some(topic_title) = data
                .get("topic")
                .and_then(|t| t.get("title"))
                .and_then(|v| v.as_str())
            {
                title_core = topic_title.trim().to_string();
            }
        }

        if title_core.is_empty() {
            if let Some(article_title) = data.get("article_title").and_then(|v| v.as_str()) {
                title_core = article_title.trim().to_string();
            }
        }

        // 描述：优先使用 summary/comment/body，其次是 memo
        let mut description = String::new();
        if let Some(summary) = data.get("summary").and_then(|v| v.as_str()) {
            description = summary.to_string();
        } else if let Some(comment) = data.get("comment").and_then(|v| v.as_str()) {
            description = comment.to_string();
        } else if let Some(comment_body) = data
            .get("comment")
            .and_then(|c| c.get("body"))
            .and_then(|v| v.as_str())
        {
            description = comment_body.to_string();
        } else if let Some(body) = data.get("body").and_then(|v| v.as_str()) {
            description = body.to_string();
        }
        if description.is_empty() && !memo.is_empty() {
            description = memo.clone();
        }

        // 根据 key 和 data 提取尽量合理的链接
        let mut link = None;

        // 文章 id（老接口形态）
        if link.is_none() {
            if let Some(article_id) = data.get("id").and_then(|v| v.as_i64()) {
                link = Some(format!("https://sspai.com/post/{}", article_id));
            }
        }

        // 某些新形态可能有 article_id 字段
        if link.is_none() {
            if let Some(article_id) = data.get("article_id").and_then(|v| v.as_i64()) {
                link = Some(format!("https://sspai.com/post/{}", article_id));
            }
        }

        // 社区主题（如 community_comment_topic）
        if link.is_none() {
            if let Some(topic_hash) = data
                .get("topic")
                .and_then(|t| t.get("id_hash"))
                .and_then(|v| v.as_str())
            {
                link = Some(format!("https://sspai.com/t/{}", topic_hash));
            }
        }

        // 默认回退到作者动态页
        if link.is_none() {
            link = Some(base_link.clone());
        }

        // 标题：昵称 + 动作 + 可选对象标题
        let title = if title_core.is_empty() {
            if !action.is_empty() {
                format!("{}{}", nickname, action)
            } else {
                format!("{} 的一条动态", nickname)
            }
        } else if !action.is_empty() {
            format!("{}{}：{}", nickname, action, title_core)
        } else {
            title_core.clone()
        };

        // 统一作者显示为该用户昵称
        let author = item
            .get("author")
            .and_then(|a| a.get("nickname"))
            .and_then(|v| v.as_str())
            .unwrap_or(&nickname)
            .to_string();

        let pub_date = crate::routes::sspai::parse_unix_to_fixed(created_at);

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link,
            author: Some(author),
            pub_date,
            categories: Vec::new(),
        });

        // 目前未强依赖 key，但保留以便后续扩展
        let _ = key;
    }

    Ok(HubData {
        title: format!("少数派用户「{}」动态更新", nickname),
        description: Some(format!("少数派用户「{}」的动态更新。", nickname)),
        link: Some(base_link),
        image: None,
        language: None,
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_SSPAI_ACTIVITY: Route = Route {
    meta: &META_SSPAI_ACTIVITY,
    handler: handler_fn,
};
