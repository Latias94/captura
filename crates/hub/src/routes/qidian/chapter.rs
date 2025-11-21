use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const MOBILE_ROOT: &str = "https://m.qidian.com";
const INFO_ROOT: &str = "https://book.qidian.com";

pub const META_QIDIAN_CHAPTER: RouteMeta = RouteMeta {
    hub_id: "qidian/chapter",
    path: "/qidian/chapter/:id",
    categories: &["reading"],
    example: "/qidian/chapter/1010400217",
    params: &[ParamMeta {
        name: "id",
        description: "Novel id from Qidian, can be found in book.qidian.com/info/:id URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["book.qidian.com/info/:id"],
        target: "/chapter/:id",
    }],
    name: "起点中文网 - 作品章节",
    maintainers: &["captura"],
    url: "https://www.qidian.com",
    description: "Qidian novel chapter list, aligned with RSSHub /qidian/chapter/:id route.",
    default_view: Some("notifications"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("qidian/chapter: id is required".to_string()))?;

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("qidian/chapter client error: {}", e)))?;

    // Scope 1: fetch and parse mobile book page (title, cover).
    let (name, image) = {
        let mobile_url = format!("{MOBILE_ROOT}/book/{id}.html");
        let resp = client
            .get(&mobile_url)
            .send()
            .await
            .map_err(|e| Error::Network(format!("qidian/chapter mobile: {}", e)))?;
        if !resp.status().is_success() {
            return Err(Error::Network(format!(
                "qidian/chapter mobile status {}",
                resp.status()
            )));
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let doc = Html::parse_document(&html);

        let sel_meta_title = Selector::parse(r#"meta[property="og:title"]"#)
            .map_err(|e| Error::Parse(e.to_string()))?;
        let sel_cover = Selector::parse(".detail__header-cover__img")
            .map_err(|e| Error::Parse(e.to_string()))?;

        let name = doc
            .select(&sel_meta_title)
            .next()
            .and_then(|el| el.value().attr("content"))
            .unwrap_or("")
            .to_string();
        let image = doc
            .select(&sel_cover)
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| {
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("https:{}", s)
                }
            });

        (name, image)
    };

    // Scope 2: fetch and parse catalog pageContext JSON.
    let page_context_json = {
        let catalog_url = format!("{MOBILE_ROOT}/book/{id}/catalog/");
        let catalog_resp = client
            .get(&catalog_url)
            .send()
            .await
            .map_err(|e| Error::Network(format!("qidian/chapter catalog: {}", e)))?;
        if !catalog_resp.status().is_success() {
            return Err(Error::Network(format!(
                "qidian/chapter catalog status {}",
                catalog_resp.status()
            )));
        }
        let catalog_html = catalog_resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let c_doc = Html::parse_document(&catalog_html);
        let sel_ctx = Selector::parse("#vite-plugin-ssr_pageContext")
            .map_err(|e| Error::Parse(e.to_string()))?;

        c_doc
            .select(&sel_ctx)
            .next()
            .map(|el| el.text().collect::<String>())
            .ok_or_else(|| Error::Parse("qidian/chapter: pageContext not found".to_string()))?
    };

    let v: serde_json::Value =
        serde_json::from_str(&page_context_json).map_err(|e| Error::Parse(e.to_string()))?;
    let mut items = Vec::new();

    if let Some(vs) = v
        .get("pageContext")
        .and_then(|pc| pc.get("pageProps"))
        .and_then(|pp| pp.get("pageData"))
        .and_then(|pd| pd.get("vs"))
        .and_then(|vs| vs.as_array())
    {
        for volume in vs {
            if let Some(cs) = volume.get("cs").and_then(|cs| cs.as_array()) {
                for c in cs {
                    let title = c
                        .get("cN")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    if title.is_empty() {
                        continue;
                    }
                    let chapter_id = c
                        .get("id")
                        .and_then(|idv| idv.as_i64())
                        .unwrap_or(0)
                        .to_string();
                    let link =
                        format!("https://vipreader.qidian.com/chapter/{}/{}", id, chapter_id);
                    let date_str = c.get("uT").and_then(|s| s.as_str()).unwrap_or("");
                    let pub_date = parse_pub_date(date_str);

                    items.push(HubItem {
                        title,
                        description: None,
                        link: Some(link),
                        author: None,
                        pub_date,
                        categories: Vec::new(),
                    });
                }
            }
        }
    }

    let info_link = format!("{INFO_ROOT}/info/{id}");

    Ok(HubData {
        title: format!("起点 {}", name),
        description: None,
        link: Some(info_link),
        image,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_QIDIAN_CHAPTER: Route = Route {
    meta: &META_QIDIAN_CHAPTER,
    handler: handler_fn,
};
