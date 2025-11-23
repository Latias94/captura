use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.techpowerup.com";

pub const META_TECHPOWERUP_INDEX: RouteMeta = RouteMeta {
    hub_id: "techpowerup/index",
    path: "/techpowerup",
    categories: &["technology"],
    example: "/techpowerup",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["techpowerup.com/"],
        target: "/",
    }],
    name: "TechPowerUp - Latest Content",
    maintainers: &["captura"],
    url: "https://www.techpowerup.com",
    description: "Latest news and reviews from TechPowerUp, roughly aligned with RSSHub /techpowerup route (simplified reviews handling).",
    default_view: Some("articles"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("techpowerup client error: {}", e)))?;

    // TechPowerUp uses a simple botcheck cookie; we set a best-effort value.
    let cookie_value = format!("botcheck={:x}", Utc::now().timestamp_millis());

    // Scope 1: fetch homepage and extract list meta.
    let items_meta = {
        let resp = client
            .get(ROOT_URL)
            .header("Cookie", &cookie_value)
            .send()
            .await
            .map_err(|e| Error::Network(format!("techpowerup index: {}", e)))?;
        if !resp.status().is_success() {
            return Err(Error::Network(format!(
                "techpowerup index: http status {}",
                resp.status()
            )));
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let doc = Html::parse_document(&html);

        let sel_post = Selector::parse(".newspost").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_h1_a = Selector::parse("h1 a").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_time = Selector::parse("time").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_author =
            Selector::parse(".byline address").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_flags =
            Selector::parse(".byline .flags span").map_err(|e| Error::Parse(e.to_string()))?;

        let mut metas = Vec::new();
        for post in doc.select(&sel_post).take(limit) {
            let a = match post.select(&sel_h1_a).next() {
                Some(a) => a,
                None => continue,
            };
            let title = a.text().collect::<String>().trim().to_string();
            let href = a.value().attr("href").unwrap_or("").trim();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let link = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{}{}", ROOT_URL, href)
            };

            let date_str = post
                .select(&sel_time)
                .next()
                .and_then(|el| el.value().attr("datetime"))
                .unwrap_or("")
                .to_string();
            let pub_date = if date_str.is_empty() {
                None
            } else {
                parse_pub_date(&date_str)
            };

            let author = post
                .select(&sel_author)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty());

            let mut categories = Vec::new();
            for flag in post.select(&sel_flags) {
                let text = flag.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    categories.push(text);
                }
            }

            metas.push((title, link, pub_date, author, categories));
        }

        metas
    };

    // Scope 2: fetch each article detail.
    let mut items = Vec::new();
    for (title, link, pub_date, author, mut categories) in items_meta {
        let detail_html = match client
            .get(&link)
            .header("Cookie", &cookie_value)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(t) => t,
                Err(_) => {
                    items.push(HubItem {
                        title,
                        description: None,
                        link: Some(link),
                        author: author.clone(),
                        pub_date,
                        categories: categories.clone(),
                    });
                    continue;
                }
            },
            _ => {
                items.push(HubItem {
                    title,
                    description: None,
                    link: Some(link),
                    author: author.clone(),
                    pub_date,
                    categories: categories.clone(),
                });
                continue;
            }
        };

        let doc = Html::parse_document(&detail_html);
        let sel_text =
            Selector::parse(".newspost .text").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_tags = Selector::parse(".tags li a").map_err(|e| Error::Parse(e.to_string()))?;

        let description = doc
            .select(&sel_text)
            .next()
            .map(|el| el.inner_html())
            .unwrap_or_default();

        for tag in doc.select(&sel_tags) {
            let text = tag.text().collect::<String>().trim().to_string();
            if !text.is_empty() && !categories.contains(&text) {
                categories.push(text);
            }
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link: Some(link),
            author: author.clone(),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: "TechPowerUp".to_string(),
        description: Some("Latest news and reviews from TechPowerUp.".to_string()),
        link: Some(ROOT_URL.to_string()),
        image: Some("https://tpucdn.com/apple-touch-icon-v1684568903519.png".to_string()),
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_TECHPOWERUP_INDEX: Route = Route {
    meta: &META_TECHPOWERUP_INDEX,
    handler: handler_fn,
};
