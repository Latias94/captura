use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://source.android.com";
const OVERVIEW_URL: &str = "https://source.android.com/docs/security/bulletin/asb-overview";

pub const META_ANDROID_SECURITY_BULLETIN: RouteMeta = RouteMeta {
    hub_id: "android/security-bulletin",
    path: "/android/security-bulletin",
    categories: &["program-update"],
    example: "/android/security-bulletin",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "source.android.com/docs/security/bulletin",
            "source.android.com/docs/security/bulletin/asb-overview",
            "source.android.com/",
        ],
        target: "/docs/security/bulletin/asb-overview",
    }],
    name: "Security Bulletins",
    maintainers: &["captura"],
    url: "https://source.android.com/docs/security/bulletin/asb-overview",
    description:
        "Android Security Bulletins overview page, aligned with RSSHub /android/security-bulletin.",
    default_view: Some("security"),
};

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(_ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("android security-bulletin client error: {}", e)))?;

    let resp = client
        .get(OVERVIEW_URL)
        .header(
            "Cookie",
            "signin=autosignin; cookies_accepted=true; django_language=en;",
        )
        .send()
        .await
        .map_err(|e| Error::Network(format!("android security-bulletin: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "android security-bulletin: http status {}",
            resp.status()
        )));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let doc = Html::parse_document(&body);
    let sel_tr = Selector::parse("table tr").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_td = Selector::parse("td").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_a = Selector::parse("a").map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for (idx, row) in doc.select(&sel_tr).enumerate() {
        if idx == 0 {
            // Skip header row.
            continue;
        }

        let tds: Vec<_> = row.select(&sel_td).collect();
        if tds.len() < 3 {
            continue;
        }

        let a = tds[0].select(&sel_a).next();
        let a_ref = match a {
            Some(v) => v,
            None => continue,
        };

        let bulletin_label = a_ref.text().collect::<String>().trim().to_string();
        if bulletin_label.is_empty() {
            continue;
        }

        let href = a_ref.value().attr("href").unwrap_or("").trim();
        if href.is_empty() {
            continue;
        }

        let link = if href.starts_with("http") {
            href.to_string()
        } else {
            format!("{}{}", BASE_URL, href)
        };

        let description_html = tds[1].inner_html();
        let date_text = tds[2].text().collect::<String>().trim().to_string();
        let pub_date = if date_text.is_empty() {
            None
        } else {
            parse_pub_date(&date_text)
        };

        let title = format!("Bulletin {}", bulletin_label);

        items.push(HubItem {
            title,
            description: Some(description_html),
            link: Some(link),
            author: None,
            pub_date,
            categories: vec!["android".to_string(), "security-bulletin".to_string()],
        });
    }

    let title = doc
        .select(&Selector::parse("title").map_err(|e| Error::Parse(e.to_string()))?)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Android Security Bulletins".to_string());

    let image = doc
        .select(
            &Selector::parse(r#"link[rel="apple-touch-icon"]"#)
                .map_err(|e| Error::Parse(e.to_string()))?,
        )
        .next()
        .and_then(|el| el.value().attr("href"))
        .map(|s| {
            let href = s.trim();
            if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{}{}", BASE_URL, href)
            }
        });

    Ok(HubData {
        title,
        description: Some("Android Security Bulletins overview.".to_string()),
        link: Some(OVERVIEW_URL.to_string()),
        image,
        language: Some("en".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_ANDROID_SECURITY_BULLETIN: Route = Route {
    meta: &META_ANDROID_SECURITY_BULLETIN,
    handler: handler_fn,
};
