use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.69shuba.cx";

pub const META_69SHU_ARTICLE: RouteMeta = RouteMeta {
    hub_id: "69shu/article",
    path: "/69shu/article/:id",
    categories: &["reading"],
    example: "/69shu/article/47117",
    params: &[ParamMeta {
        name: "id",
        description: "Novel id from 69shu book URL, e.g. 47117 for /book/47117.htm",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.69shuba.cx"],
        target: "/article/:id",
    }],
    name: "69书吧 - 章节",
    maintainers: &["captura"],
    url: "https://www.69shuba.cx",
    description:
        "69shuba latest chapter list, roughly aligned with RSSHub /69shu/article/:id route. Encryption-specific reordering is not applied.",
    default_view: Some("articles"),
};

fn decode_gbk(bytes: &[u8]) -> Result<String, Error> {
    let (cow, _, had_errors) = encoding_rs::GBK.decode(bytes);
    if had_errors {
        return Err(Error::Parse("69shu: GBK decode error".to_string()));
    }
    Ok(cow.into_owned())
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("69shu/article: id is required".to_string()))?;

    let book_url = format!("{ROOT_URL}/book/{}.htm", id);
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("69shu/article client error: {}", e)))?;
    let resp = client
        .get(&book_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("69shu/article: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "69shu/article: http status {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let html = decode_gbk(&bytes)?;
    // Parse book metadata and chapter list in a separate scope so that
    // the non-Send Html document does not live across await points.
    let (book_title, book_desc, book_image, author, chapters) = {
        let doc = Html::parse_document(&html);

        let sel_title = Selector::parse("h1 > a").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_desc = Selector::parse(".navtxt > p").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_image =
            Selector::parse(".bookimg2 img").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_author =
            Selector::parse(".booknav2 p > a").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_chapters =
            Selector::parse(".qustime li > a").map_err(|e| Error::Parse(e.to_string()))?;

        let book_title = doc
            .select(&sel_title)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| format!("69shu {}", id));
        let book_desc = doc
            .select(&sel_desc)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        let book_image = doc
            .select(&sel_image)
            .next()
            .and_then(|img| img.value().attr("src"))
            .map(|s| s.to_string());
        let author = doc
            .select(&sel_author)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        let mut chapters = Vec::new();
        for a in doc.select(&sel_chapters) {
            let title = a.text().collect::<String>().trim().to_string();
            let href = a.value().attr("href").unwrap_or("").trim();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let link = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{ROOT_URL}{}", href)
            };
            chapters.push((title, link));
        }

        (book_title, book_desc, book_image, author, chapters)
    };

    let mut items = Vec::new();
    for (title, link) in chapters {
        let detail_resp = match client.get(&link).send().await {
            Ok(r) => r,
            Err(_) => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: Some(author.clone()),
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };
        if !detail_resp.status().is_success() {
            items.push(HubItem {
                title,
                description: None,
                link: Some(link),
                author: Some(author.clone()),
                pub_date: None,
                categories: Vec::new(),
            });
            continue;
        }
        let bytes = match detail_resp.bytes().await {
            Ok(b) => b,
            Err(_) => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: Some(author.clone()),
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };
        let detail_html = match decode_gbk(&bytes) {
            Ok(h) => h,
            Err(_) => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: Some(author.clone()),
                    pub_date: None,
                    categories: Vec::new(),
                });
                continue;
            }
        };
        let detail = Html::parse_document(&detail_html);
        let sel_body = Selector::parse(".txtnav").map_err(|e| Error::Parse(e.to_string()))?;

        let description = detail
            .select(&sel_body)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: Some(author.clone()),
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title: book_title,
        description: Some(book_desc),
        link: Some(book_url),
        image: book_image,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_69SHU_ARTICLE: Route = Route {
    meta: &META_69SHU_ARTICLE,
    handler: handler_fn,
};
