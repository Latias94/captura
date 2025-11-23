use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

const ROOT_URL: &str = "https://www.aicaijing.com.cn";
const API_ROOT_URL: &str = "https://api.aicaijing.com.cn";

#[derive(Debug, Deserialize)]
struct ApiResponse {
    data: ApiData,
}

#[derive(Debug, Deserialize)]
struct ApiData {
    items: Vec<ArticleItem>,
}

#[derive(Debug, Deserialize)]
struct ArticleItem {
    #[serde(rename = "articleId")]
    article_id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    createTime: String,
    #[serde(default)]
    cover: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    #[serde(rename = "category")]
    category_obj: CategoryObj,
    #[serde(default)]
    userInfo: UserInfo,
    #[serde(default)]
    tags: Vec<TagItem>,
}

#[derive(Debug, Deserialize, Default)]
struct CategoryObj {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct UserInfo {
    #[serde(default)]
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct TagItem {
    #[serde(default)]
    name: String,
}

fn parse_pub_date(raw: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(raw)
}

pub const META_AICAIJING_INDEX: RouteMeta = RouteMeta {
    hub_id: "aicaijing/index",
    path: "/aicaijing/:category?/:id?",
    categories: &["new-media"],
    example: "/aicaijing/latest",
    params: &[
        ParamMeta {
            name: "category",
            description: "栏目类型：latest（最新）、recommend（推荐）、cover（封面）、information（按分类 id）。",
            default: Some("latest"),
            options: &[
                ("latest", "最新文章"),
                ("recommend", "推荐资讯"),
                ("cover", "封面文章"),
                ("information", "按分类 id 过滤"),
            ],
        },
        ParamMeta {
            name: "id",
            description: "当 category=information 时的分类 id，例如 14 表示“热点-最新”，5 表示“热点-科技”等。",
            default: Some("14"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["aicaijing.com.cn"],
        target: "/:category?/:id?",
    }],
    name: "AI 财经社 - 新闻与深度",
    maintainers: &["captura"],
    url: "https://www.aicaijing.com.cn",
    description: "AI 财经社热点与深度文章列表，对齐 RSSHub /aicaijing/:category?/:id? 路由。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let category = ctx.param_str("category").unwrap_or("latest");
    let id_str = ctx.param_str("id").unwrap_or("14");
    let id: i64 = id_str.parse().unwrap_or(14);

    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let titles = [
        (14, "热点 - 最新"),
        (5, "热点 - 科技"),
        (9, "热点 - 消费"),
        (7, "热点 - 出行"),
        (13, "热点 - 文娱"),
        (10, "热点 - 教育"),
        (25, "热点 - 地产"),
        (11, "热点 - 更多"),
        (28, "深度 - 出行"),
        (29, "深度 - 科技"),
        (31, "深度 - 消费"),
        (33, "深度 - 教育"),
        (34, "深度 - 更多"),
        (8, "深度 - 地产"),
        (6, "深度 - 文娱"),
    ];

    fn lookup_title(id: i64, titles: &[(i64, &str)]) -> &'static str {
        for (tid, title) in titles {
            if *tid == id {
                // Safety: all literals have 'static lifetime.
                return Box::leak(title.to_string().into_boxed_str());
            }
        }
        "资讯"
    }

    let (url_suffix, title_part) = match category {
        "latest" => ("", "最新文章"),
        "recommend" => ("&isRecommend=true", "推荐资讯"),
        "cover" => ("&position=1", "封面文章"),
        "information" => ("", lookup_title(id, &titles)),
        _ => ("", "最新文章"),
    };

    let api_url = if category == "information" {
        format!(
            "{}/article/detail/list?size={}&page=1&categoryId={}",
            API_ROOT_URL, limit, id
        )
    } else {
        format!(
            "{}/article/detail/list?size={}&page=1{}",
            API_ROOT_URL, limit, url_suffix
        )
    };

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("aicaijing client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{} -> {}", api_url, e)))?;

    let api: ApiResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("aicaijing json parse error: {}", e)))?;

    let mut items = Vec::new();

    for item in api.data.items.into_iter().take(limit) {
        let link = format!("{}/article/{}", ROOT_URL, item.article_id);
        let pub_date = parse_pub_date(&item.createTime);
        let mut categories = Vec::new();
        if !item.category_obj.name.is_empty() {
            categories.push(item.category_obj.name.clone());
        }
        for tag in &item.tags {
            if !tag.name.is_empty() {
                categories.push(tag.name.clone());
            }
        }

        let mut description = String::new();
        if !item.cover.is_empty() {
            description.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = item.cover,
                alt = item.title
            ));
        }
        if !item.content.is_empty() {
            description.push_str(&item.content);
        }

        items.push(HubItem {
            title: item.title.trim().to_string(),
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: Some(item.userInfo.nickname.trim().to_string()),
            pub_date,
            categories,
        });
    }

    let feed_title = format!("AI 财经社 - {}", title_part);

    Ok(HubData {
        title: feed_title.clone(),
        description: Some(feed_title),
        link: Some(ROOT_URL.to_string()),
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
pub const ROUTE_AICAIJING_INDEX: Route = Route {
    meta: &META_AICAIJING_INDEX,
    handler: handler_fn,
};
