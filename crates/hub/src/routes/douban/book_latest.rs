use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;

const DOUBAN_MOBILE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 11_0 like Mac OS X) AppleWebKit/604.1.38 (KHTML, like Gecko) Version/11.0 Mobile/15A372 Safari/604.1";

pub const META_DOUBAN_BOOK_LATEST: RouteMeta = RouteMeta {
    hub_id: "douban/book-latest",
    path: "/douban/book/latest/:kind?",
    categories: &["social-media"],
    example: "/douban/book/latest/fiction",
    params: &[ParamMeta {
        name: "kind",
        description:
            "专题分类，默认 all，可选：all, prose_poetry, fiction, history, biography, science, art, business, comics",
        default: Some("all"),
        options: &[
            ("all", "全部"),
            ("prose_poetry", "文学"),
            ("fiction", "小说"),
            ("history", "历史文化"),
            ("biography", "社会纪实"),
            ("science", "科学新知"),
            ("art", "艺术设计"),
            ("business", "商业经管"),
            ("comics", "绘本漫画"),
        ],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["book.douban.com"],
        target: "/latest",
    }],
    name: "Douban New Books",
    maintainers: &["captura"],
    url: "https://book.douban.com/latest",
    description: "豆瓣新书速递，对标 RSSHub /douban/book/latest/:type 路由。",
    default_view: Some("books"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let kind = ctx.param_str("kind").unwrap_or("all");
    let (subcat_label, sub_url) = match kind {
        "prose_poetry" => ("文学", "new_book_prose_poetry"),
        "fiction" => ("小说", "new_book_fiction"),
        "history" => ("历史文化", "new_book_history"),
        "biography" => ("社会纪实", "new_book_biography"),
        "science" => ("科学新知", "new_book_science"),
        "art" => ("艺术设计", "new_book_art"),
        "business" => ("商业经管", "new_book_business"),
        "comics" => ("绘本漫画", "new_book_comics"),
        _ => ("全部", "new_book_all"),
    };

    let api_url = format!(
        "https://m.douban.com/rexxar/api/v2/subject_collection/{}/items?start=0&count=10&mode=collection&for_mobile=1",
        sub_url
    );

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("douban book latest client: {}", e)))?;
    let resp = client
        .get(&api_url)
        .header("Referer", "https://book.douban.com/latest")
        .header("User-Agent", DOUBAN_MOBILE_UA)
        .send()
        .await
        .map_err(|e| Error::Network(format!("{api_url} -> {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!("{api_url} -> http status {status}")));
    }

    let api_resp: DoubanNewBookResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("douban book latest json: {e}")))?;

    let mut items = Vec::new();
    for item in api_resp.items {
        let title = item.title.clone();
        let url = item.url.clone();

        let cover_url = item
            .pic
            .as_ref()
            .and_then(|p| p.normal.as_ref())
            .cloned()
            .unwrap_or_default();

        let rate = match item.rating {
            Some(r) if r.value > 0.0 => format!("{:.1}分", r.value),
            _ => item.null_rating_reason.unwrap_or_default(),
        };

        let info = item.card_subtitle.unwrap_or_default();
        let extra = item
            .cards
            .as_ref()
            .and_then(|cards| cards.get(0))
            .and_then(|c| c.content.as_deref())
            .unwrap_or("");

        let mut desc = String::new();
        if !cover_url.is_empty() {
            desc.push_str(&format!(r#"<img src="{}"><br>"#, cover_url));
        }
        desc.push_str(&title);
        desc.push_str("<br><br>");
        if !info.is_empty() {
            desc.push_str(&info);
            desc.push_str("<br><br>");
        }
        if !extra.is_empty() {
            desc.push_str(extra);
            desc.push_str("<br><br>");
        }
        if !rate.is_empty() {
            desc.push_str(&rate);
        }

        items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(url),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    let mut title = "豆瓣新书速递".to_string();
    if kind != "all" {
        title.push('-');
        title.push_str(subcat_label);
    }
    let mut link = "https://book.douban.com/latest".to_string();
    if kind != "all" {
        link.push_str("?subcat=");
        link.push_str(subcat_label);
    }

    Ok(HubData {
        title: title.clone(),
        description: Some(title),
        link: Some(link),
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
pub const ROUTE_DOUBAN_BOOK_LATEST: Route = Route {
    meta: &META_DOUBAN_BOOK_LATEST,
    handler: handler_fn,
};

#[derive(Debug, Deserialize)]
struct DoubanNewBookResponse {
    items: Vec<DoubanNewBookItem>,
}

#[derive(Debug, Deserialize)]
struct DoubanNewBookItem {
    title: String,
    url: String,
    #[serde(default)]
    card_subtitle: Option<String>,
    #[serde(default)]
    cards: Option<Vec<DoubanBookCard>>,
    pic: Option<DoubanPic>,
    #[serde(default)]
    rating: Option<DoubanRating>,
    #[serde(default)]
    null_rating_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanBookCard {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanPic {
    #[serde(default)]
    normal: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubanRating {
    #[serde(default)]
    value: f64,
}
