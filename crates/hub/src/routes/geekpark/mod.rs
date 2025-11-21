use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, TimeZone};
use serde::Deserialize;

const ROOT_URL: &str = "https://geekpark.net";
const API_ROOT: &str = "https://mainssl.geekpark.net";

#[derive(Debug, Deserialize)]
struct HomeResp {
    #[serde(default)]
    homepage_posts: Vec<HomePost>,
}

#[derive(Debug, Deserialize)]
struct HomePost {
    #[serde(default)]
    post: Option<ApiPost>,
}

#[derive(Debug, Deserialize)]
struct ColumnResp {
    column: ColumnMeta,
}

#[derive(Debug, Deserialize)]
struct ColumnMeta {
    title: String,
    description: String,
    banner_url: String,
    #[serde(default)]
    posts: Vec<ApiPost>,
}

#[derive(Debug, Deserialize)]
struct ApiPost {
    id: i64,
    title: String,
    #[serde(default, rename = "abstract")]
    abstract_: Option<String>,
    #[serde(default)]
    cover_url: String,
    published_timestamp: i64,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    column: Option<ApiColumn>,
    #[serde(default)]
    authors: Vec<ApiAuthor>,
}

#[derive(Debug, Deserialize)]
struct ApiColumn {
    title: String,
}

#[derive(Debug, Deserialize)]
struct ApiAuthor {
    #[serde(default)]
    realname: String,
    #[serde(default)]
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct PostDetailResp {
    post: PostDetail,
}

#[derive(Debug, Deserialize)]
struct PostDetail {
    id: i64,
    title: String,
    #[serde(default, rename = "abstract")]
    abstract_: Option<String>,
    content: String,
    #[serde(default)]
    cover_url: String,
    published_timestamp: i64,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    column: Option<ApiColumn>,
    #[serde(default)]
    authors: Vec<ApiAuthor>,
}

fn parse_timestamp(ts: i64) -> Option<DateTime<FixedOffset>> {
    // API 返回秒级 Unix 时间戳
    let naive = chrono::NaiveDateTime::from_timestamp_opt(ts, 0)?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_utc_datetime(&naive))
}

fn join_authors(authors: &[ApiAuthor]) -> String {
    let mut names = Vec::new();
    for a in authors {
        if !a.realname.trim().is_empty() {
            names.push(a.realname.trim().to_string());
        } else if !a.nickname.trim().is_empty() {
            names.push(a.nickname.trim().to_string());
        }
    }
    names.join("/")
}

fn build_categories(tags: &[String], column: &Option<ApiColumn>) -> Vec<String> {
    let mut cats = Vec::new();
    if let Some(c) = column {
        if !c.title.trim().is_empty() {
            cats.push(c.title.trim().to_string());
        }
    }
    for t in tags {
        if !t.trim().is_empty() {
            cats.push(t.trim().to_string());
        }
    }
    cats.sort();
    cats.dedup();
    cats
}

pub const META_GEEKPARK: RouteMeta = RouteMeta {
    hub_id: "geekpark",
    path: "/geekpark/:column?",
    categories: &["technology"],
    example: "/geekpark/304",
    params: &[ParamMeta {
        name: "column",
        description: "栏目 id，空为首页资讯；如综合报道 179、AI新浪潮观察 304 等。",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["geekpark.net", "www.geekpark.net"],
        target: "/:column?",
    }],
    name: "极客公园栏目",
    maintainers: &["captura"],
    url: "https://www.geekpark.net",
    description:
        "极客公园首页或指定栏目文章列表，对标 RSSHub /geekpark/:column 路由（含 AI 新浪潮等栏目）。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let column = ctx.param_str("column");
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let current_url = if let Some(col) = column {
        format!("{}/column/{}", ROOT_URL, col)
    } else {
        ROOT_URL.to_string()
    };

    let api_url = if let Some(col) = column {
        format!("{}/api/v1/columns/{}", API_ROOT, col)
    } else {
        format!("{}/api/v2", API_ROOT)
    };

    // GeekPark 的列表示例中偶尔会返回 502，通过浏览器 UA 可显著降低风控几率。
    let client = captura_net::client_basic(
        Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Captura/Geekkpark".to_string()),
        None,
    )
        .map_err(|e| Error::Network(format!("geekpark client error: {}", e)))?;
    let resp = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }

    let mut items = Vec::new();
    let mut title = String::new();
    let mut description = String::new();
    let mut image = None;

    if column.is_some() {
        let data: ColumnResp = resp
            .json()
            .await
            .map_err(|e| Error::Parse(format!("geekpark column json parse: {e}")))?;
        title = format!("{} | 极客公园", data.column.title);
        description = data.column.description.clone();
        if !data.column.banner_url.trim().is_empty() {
            image = Some(data.column.banner_url.clone());
        }
        for post in data.column.posts.into_iter().take(limit) {
            let base = build_item_from_post(&post)?;
            let detail = fetch_detail(post.id).await.unwrap_or(None);
            items.push(merge_with_detail(base, detail));
        }
    } else {
        let data: HomeResp = resp
            .json()
            .await
            .map_err(|e| Error::Parse(format!("geekpark home json parse: {e}")))?;
        for entry in data.homepage_posts.into_iter().take(limit) {
            let post = entry.post.unwrap_or(ApiPost {
                id: 0,
                title: String::new(),
                abstract_: None,
                cover_url: String::new(),
                published_timestamp: 0,
                tags: Vec::new(),
                column: None,
                authors: Vec::new(),
            });
            if post.id == 0 {
                continue;
            }
            let base = build_item_from_post(&post)?;
            let detail = fetch_detail(post.id).await.unwrap_or(None);
            items.push(merge_with_detail(base, detail));
        }

        // 补全首页元信息（从 HTML 抓 title/description/image）
        if let Ok(html) = util::get_html(&current_url).await {
            use scraper::{Html, Selector};
            let doc = Html::parse_document(&html);
            if let Ok(sel) = Selector::parse("title") {
                if let Some(el) = doc.select(&sel).next() {
                    let t = el.text().collect::<String>().trim().to_string();
                    if !t.is_empty() {
                        title = t;
                    }
                }
            }
            if let Ok(sel) = Selector::parse("meta[property='og:description']") {
                if let Some(el) = doc.select(&sel).next() {
                    if let Some(content) = el.value().attr("content") {
                        if !content.trim().is_empty() {
                            description = content.trim().to_string();
                        }
                    }
                }
            }
            if let Ok(sel) = Selector::parse("meta[name='og:image']") {
                if let Some(el) = doc.select(&sel).next() {
                    if let Some(content) = el.value().attr("content") {
                        if !content.trim().is_empty() {
                            let url = if content.starts_with("http") {
                                content.to_string()
                            } else {
                                format!("https:{}", content)
                            };
                            image = Some(url);
                        }
                    }
                }
            }
        }
    }

    if title.is_empty() {
        title = "极客公园".to_string();
    }

    Ok(HubData {
        title,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        link: Some(current_url),
        image,
        language: None,
        items,
        allow_empty: true,
    })
}

fn build_item_from_post(post: &ApiPost) -> captura_common::Result<HubItem> {
    let title = post.title.trim().to_string();
    if title.is_empty() {
        return Err(Error::Parse("geekpark: empty title".into()));
    }
    let link = format!("{}/api/v1/posts/{}", API_ROOT, post.id);
    let author_name = join_authors(&post.authors);
    let pub_date = parse_timestamp(post.published_timestamp);
    let categories = build_categories(&post.tags, &post.column);
    let mut description = String::new();
    if let Some(intro) = &post.abstract_ {
        description.push_str(intro);
    }
    Ok(HubItem {
        title,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        link: Some(link),
        author: if author_name.is_empty() {
            None
        } else {
            Some(author_name)
        },
        pub_date,
        categories,
    })
}

async fn fetch_detail(id: i64) -> captura_common::Result<Option<PostDetail>> {
    let url = format!("{}/api/v1/posts/{}", API_ROOT, id);
    let client = captura_net::client_basic(
        Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Captura/Geekkpark".to_string()),
        None,
    )
        .map_err(|e| Error::Network(format!("geekpark detail client: {}", e)))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{url} -> {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Ok(None);
    }
    let detail: PostDetailResp = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("geekpark detail json parse: {e}")))?;
    Ok(Some(detail.post))
}

fn merge_with_detail(mut base: HubItem, detail: Option<PostDetail>) -> HubItem {
    if let Some(d) = detail {
        if !d.title.trim().is_empty() {
            base.title = d.title.trim().to_string();
        }
        let mut desc = String::new();
        if let Some(intro) = &d.abstract_ {
            desc.push_str(intro);
            desc.push_str("<br><br>");
        }
        desc.push_str(&d.content);
        if !desc.trim().is_empty() {
            base.description = Some(desc);
        }
        base.link = Some(format!("{}/news/{}", ROOT_URL, d.id));
        let author_name = join_authors(&d.authors);
        if !author_name.is_empty() {
            base.author = Some(author_name);
        }
        let cats = build_categories(&d.tags, &d.column);
        if !cats.is_empty() {
            base.categories = cats;
        }
        base.pub_date = parse_timestamp(d.published_timestamp);
    }
    base
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GEEKPARK: Route = Route {
    meta: &META_GEEKPARK,
    handler: handler_fn,
};
