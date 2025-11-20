use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

pub const META_JUEJIN_TRENDING: RouteMeta = RouteMeta {
    hub_id: "juejin/trending",
    path: "/juejin/trending/:category/:type",
    categories: &["programming"],
    example: "/juejin/trending/all/weekly",
    params: &[
        ParamMeta {
            name: "category",
            description: "Category slug in Juejin (e.g. frontend, backend, ios, android, ai, freebie, article, all)",
            default: Some("all"),
            options: &[
                ("all", "全部"),
                ("frontend", "前端"),
                ("backend", "后端"),
                ("android", "Android"),
                ("ios", "iOS"),
                ("ai", "人工智能"),
                ("freebie", "开发工具"),
                ("article", "阅读"),
                ("devops", "运维"),
                ("product", "产品"),
                ("design", "设计"),
            ],
        },
        ParamMeta {
            name: "type",
            description: "Time range type: weekly / monthly / historical",
            default: Some("weekly"),
            options: &[
                ("weekly", "本周最热"),
                ("monthly", "本月最热"),
                ("historical", "历史最热"),
            ],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["juejin.cn"],
        target: "/",
    }],
    name: "Juejin Trending",
    maintainers: &["captura"],
    url: "https://juejin.cn",
    description: "掘金热门文章，参考 RSSHub /juejin/trending 实现。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("all");
    let type_param = ctx.param_str("type").unwrap_or("weekly");

    let type_cfg = match type_param {
        "monthly" => TrendingTypeConfig {
            period: "month",
            title: "本月",
            link: "monthly_hottest",
            sort_type: 30,
        },
        "historical" => TrendingTypeConfig {
            period: "",
            title: "历史",
            link: "hottest",
            sort_type: 0,
        },
        _ => TrendingTypeConfig {
            period: "week",
            title: "本周",
            link: "weekly_hottest",
            sort_type: 7,
        },
    };

    let mut cate_name = String::new();
    let mut cate_url_slug = "recommended".to_string();
    let mut cate_id: Option<String> = None;

    if let Some(cat) = lookup_category(category) {
        cate_name = cat.name.to_string();
        cate_url_slug = cat.url.to_string();
        cate_id = Some(cat.id.to_string());
    }

    if cate_url_slug.is_empty() {
        cate_url_slug = "recommended".to_string();
    }

    let title = if cate_name.is_empty() {
        format!("掘金{}最热", type_cfg.title)
    } else {
        format!("掘金{}{}最热", cate_name, type_cfg.title)
    };

    let list_path = &cate_url_slug;
    let feed_link = format!("https://juejin.cn/{}?sort={}", list_path, type_cfg.link);

    let mut body = serde_json::json!({
        "cursor": "0",
        "id_type": 2,
        "limit": 20,
        "sort_type": type_cfg.sort_type,
    });

    let api_url = if cate_url_slug == "recommended" {
        "https://api.juejin.cn/recommend_api/v1/article/recommend_all_feed"
    } else {
        if let Some(id) = cate_id {
            body["cate_id"] = serde_json::Value::String(id);
        }
        "https://api.juejin.cn/recommend_api/v1/article/recommend_cate_feed"
    };

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("juejin client: {}", e)))?;
    let resp = client
        .post(api_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{api_url} -> http status {status}"
        )));
    }

    let body_text = resp
        .text()
        .await
        .map_err(|e| Error::Parse(format!("juejin trending body: {e}")))?;

    let api_resp: TrendingApiResponse = serde_json::from_str(&body_text).map_err(|e| {
        let snippet = if body_text.len() > 512 {
            &body_text[..512]
        } else {
            &body_text
        };
        Error::Parse(format!("juejin trending json decode: {e}; snippet={}", snippet))
    })?;

    if api_resp.err_no != 0 {
        return Err(Error::Network(format!(
            "juejin trending err_no={} err_msg={}",
            api_resp.err_no,
            api_resp.err_msg.unwrap_or_default()
        )));
    }

    let mut items = Vec::new();
    for entry in api_resp.data {
        let (item_type, item_info) = match entry {
            TrendingItem::Wrapped { item_type, item_info } => (item_type, item_info),
            TrendingItem::Direct(item_info) => (2, item_info),
        };

        if item_type != 2 {
            continue;
        }

        let info = match item_info.article_info {
            Some(info) => info,
            None => continue,
        };

        if info.title.trim().is_empty() {
            continue;
        }

        let article_id = match item_info.article_id {
            Some(id) if !id.is_empty() => id,
            _ => info.article_id.clone().unwrap_or_default(),
        };

        let link = if article_id.is_empty() {
            None
        } else {
            Some(format!("https://juejin.cn/post/{}", article_id))
        };

        let author = item_info
            .author_user_info
            .map(|a| a.user_name)
            .filter(|s| !s.is_empty());

        let mut categories = Vec::new();
        if let Some(cat) = item_info.category {
            if !cat.category_name.is_empty() {
                categories.push(cat.category_name);
            }
        }
        if let Some(tags) = item_info.tags {
            for t in tags {
                if !t.tag_name.is_empty() {
                    categories.push(t.tag_name);
                }
            }
        }

        let pub_date = info
            .ctime
            .as_deref()
            .and_then(parse_unix_timestamp_to_fixed);

        items.push(HubItem {
            title: info.title,
            description: Some(info.brief_content.unwrap_or_else(|| "无描述".to_string())),
            link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: title.clone(),
        description: Some(title),
        link: Some(feed_link),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JUEJIN_TRENDING: Route = Route {
    meta: &META_JUEJIN_TRENDING,
    handler: handler_fn,
};

struct TrendingTypeConfig {
    #[allow(dead_code)]
    period: &'static str,
    title: &'static str,
    link: &'static str,
    sort_type: i32,
}

#[derive(Debug, Deserialize)]
struct StaticCategory {
    id: &'static str,
    name: &'static str,
    url: &'static str,
}

fn lookup_category(slug: &str) -> Option<StaticCategory> {
    match slug {
        "backend" => Some(StaticCategory {
            id: "6809637769959178254",
            name: "后端",
            url: "backend",
        }),
        "frontend" => Some(StaticCategory {
            id: "6809637767543259144",
            name: "前端",
            url: "frontend",
        }),
        "android" => Some(StaticCategory {
            id: "6809635626879549454",
            name: "Android",
            url: "android",
        }),
        "ios" => Some(StaticCategory {
            id: "6809635626661445640",
            name: "iOS",
            url: "ios",
        }),
        "ai" => Some(StaticCategory {
            id: "6809637773935378440",
            name: "人工智能",
            url: "ai",
        }),
        "freebie" => Some(StaticCategory {
            id: "6809637771511070734",
            name: "开发工具",
            url: "freebie",
        }),
        "article" => Some(StaticCategory {
            id: "6809637772874219534",
            name: "阅读",
            url: "article",
        }),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct TrendingApiResponse {
    err_no: i32,
    err_msg: Option<String>,
    data: Vec<TrendingItem>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TrendingItem {
    Wrapped {
        item_type: i32,
        item_info: TrendingItemInfo,
    },
    Direct(TrendingItemInfo),
}

#[derive(Debug, Deserialize)]
struct TrendingItemInfo {
    article_id: Option<String>,
    article_info: Option<JuejinArticleInfo>,
    author_user_info: Option<JuejinAuthorInfo>,
    category: Option<JuejinCategory>,
    tags: Option<Vec<JuejinTag>>,
}

#[derive(Debug, Deserialize)]
struct JuejinArticleInfo {
    #[serde(default)]
    article_id: Option<String>,
    title: String,
    #[serde(default)]
    brief_content: Option<String>,
    ctime: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JuejinAuthorInfo {
    user_name: String,
}

#[derive(Debug, Deserialize)]
struct JuejinCategory {
    category_name: String,
}

#[derive(Debug, Deserialize)]
struct JuejinTag {
    tag_name: String,
}

fn parse_unix_timestamp_to_fixed(input: &str) -> Option<DateTime<FixedOffset>> {
    let secs: i64 = input.trim().parse().ok()?;
    let offset = FixedOffset::east_opt(0)?;
    offset.timestamp_opt(secs, 0).single()
}
