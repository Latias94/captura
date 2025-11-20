use crate::routes::types::{Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

const DOUBAN_MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_0 like Mac OS X) AppleWebKit/604.1.38 (KHTML, like Gecko) Version/11.0 Mobile/15A372 Safari/604.1";

pub const META_DOUBAN_BOOK_RANK: RouteMeta = RouteMeta {
    hub_id: "douban/book-rank",
    path: "/douban/book/rank/:kind?",
    categories: &["social-media"],
    example: "/douban/book/rank/fiction",
    params: &[ParamMeta {
        name: "kind",
        description: "图书类型：fiction（虚构）/ nonfiction（非虚构），为空则合并两类。",
        default: Some(""),
        options: &[("fiction", "虚构"), ("nonfiction", "非虚构")],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["m.douban.com"],
        target: "/book/",
    }],
    name: "Douban Book Rank",
    maintainers: &["captura"],
    url: "https://m.douban.com/book/",
    description: "豆瓣热门图书排行，对标 RSSHub /douban/book/rank/:type 路由。",
    default_view: Some("books"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let kind = ctx.param_str("kind").unwrap_or("").trim().to_string();

    let referer = if kind.is_empty() {
        "https://m.douban.com/book/".to_string()
    } else {
        format!("https://m.douban.com/book/{}", kind)
    };

    let items = if kind.is_empty() {
        let mut all = Vec::new();
        all.extend(fetch_rank_items("fiction", &referer).await?);
        all.extend(fetch_rank_items("nonfiction", &referer).await?);
        all
    } else {
        fetch_rank_items(&kind, &referer).await?
    };

    let title = if kind.is_empty() {
        "豆瓣热门图书-全部".to_string()
    } else if kind == "fiction" {
        "豆瓣热门图书-虚构类".to_string()
    } else {
        "豆瓣热门图书-非虚构类".to_string()
    };

    Ok(HubData {
        title: title.clone(),
        description: Some("每周一更新".to_string()),
        link: Some(referer),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

async fn fetch_rank_items(kind: &str, referer: &str) -> Result<Vec<HubItem>, Error> {
    let api_url = format!(
        "https://m.douban.com/rexxar/api/v2/subject_collection/book_{}/items?start=0&count=10",
        kind
    );

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("douban book rank client: {}", e)))?;
    let resp = client
        .get(&api_url)
        .header("Referer", referer)
        .header("User-Agent", DOUBAN_MOBILE_UA)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "{api_url} -> http status {status}"
        )));
    }

    let api_resp: DoubanBookRankResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("douban book rank json: {e}")))?;

    let mut out = Vec::new();
    for item in api_resp.subject_collection_items {
        let title = item.title.clone();
        let info = item.info.unwrap_or_default();
        let url = item.url.clone();
        let cover_url = item
            .cover
            .as_ref()
            .and_then(|c| c.url.as_ref())
            .cloned()
            .unwrap_or_default();

        let rate = match item.rating {
            Some(r) if r.value > 0.0 => format!("{:.1}分", r.value),
            _ => item.null_rating_reason.unwrap_or_default(),
        };

        let mut desc = String::new();
        if !cover_url.is_empty() {
            desc.push_str(&format!(r#"<img src="{}"><br>"#, cover_url));
        }
        desc.push_str(&title);
        if !info.is_empty() {
            desc.push('/');
            desc.push_str(&info);
        }
        if !rate.is_empty() {
            desc.push('/');
            desc.push_str(&rate);
        }

        out.push(HubItem {
            title: format!("{}-{}", title, info),
            description: Some(desc),
            link: Some(url),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(out)
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DOUBAN_BOOK_RANK: Route = Route {
    meta: &META_DOUBAN_BOOK_RANK,
    handler: handler_fn,
};

#[derive(Debug, Deserialize)]
struct DoubanBookRankResponse {
    subject_collection_items: Vec<DoubanBookRankItem>,
}

#[derive(Debug, Deserialize)]
struct DoubanBookRankItem {
    title: String,
    url: String,
    #[serde(default)]
    cover: Option<DoubanCover>,
    #[serde(default)]
    info: Option<String>,
    #[serde(default)]
    rating: Option<DoubanRating>,
    #[serde(default)]
    null_rating_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanCover {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanRating {
    #[serde(default)]
    value: f64,
}

