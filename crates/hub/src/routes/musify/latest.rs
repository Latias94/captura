use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://musify.club";

pub const META_MUSIFY_LATEST: RouteMeta = RouteMeta {
    hub_id: "musify/latest",
    path: "/musify/:language?",
    categories: &["multimedia"],
    example: "/musify/en",
    params: &[ParamMeta {
        name: "language",
        description: "Language path segment: empty for Russian, 'en' for English.",
        default: Some(""),
        options: &[("", "Russian"), ("en", "English")],
    }],
    features: Features::basic(),
    radar: &[
        Radar {
            source: &["musify.club/:language"],
            target: "/:language",
        },
        Radar {
            source: &["musify.club/en"],
            target: "/en",
        },
        Radar {
            source: &["musify.club"],
            target: "/",
        },
    ],
    name: "Musify Latest",
    maintainers: &["captura"],
    url: "https://musify.club",
    description: "Latest tracks list from Musify, aligned with RSSHub /musify/:language? route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let language = ctx.param_str("language").unwrap_or("");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let path = if language.is_empty() {
        "".to_string()
    } else {
        format!("/{language}")
    };
    let url = format!("{ROOT_URL}{path}");

    let html = util::get_html(&url).await?;
    let doc = Html::parse_document(&html);

    let sel_item = Selector::parse("div.playlist__item")
        .map_err(|e| captura_common::Error::Parse(format!("musify: item selector error: {e}")))?;
    let sel_heading = Selector::parse("div.playlist__heading a").map_err(|e| {
        captura_common::Error::Parse(format!("musify: heading selector error: {e}"))
    })?;
    let sel_link = Selector::parse("a.strong")
        .map_err(|e| captura_common::Error::Parse(format!("musify: link selector error: {e}")))?;
    let sel_control = Selector::parse("div.playlist__control").map_err(|e| {
        captura_common::Error::Parse(format!("musify: control selector error: {e}"))
    })?;

    let mut items = Vec::new();

    for el in doc.select(&sel_item).take(limit) {
        let artist = el.value().attr("data-artist").unwrap_or("").trim();
        let name = el.value().attr("data-name").unwrap_or("").trim();
        if artist.is_empty() && name.is_empty() {
            continue;
        }
        let title = if artist.is_empty() {
            name.to_string()
        } else if name.is_empty() {
            artist.to_string()
        } else {
            format!("{} - {}", artist, name)
        };

        let link = el
            .select(&sel_link)
            .next()
            .and_then(|a| a.value().attr("href"))
            .map(|href| util::absolutize(ROOT_URL, href));

        let mut authors = Vec::new();
        for a in el.select(&sel_heading) {
            let name = a.text().collect::<String>().trim().to_string();
            if name.is_empty() {
                continue;
            }
            authors.push(name);
        }
        let author = if authors.is_empty() {
            None
        } else {
            Some(authors.join(", "))
        };

        let enclosure_url = el
            .select(&sel_control)
            .next()
            .and_then(|c| c.value().attr("data-play-url"))
            .map(|s| util::absolutize(ROOT_URL, s));

        let mut description = String::new();
        if let Some(ref url_audio) = enclosure_url {
            description.push_str(&format!(
                "<p><audio controls src=\"{src}\">Your browser does not support the audio element.</audio></p>",
                src = url_audio
            ));
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link,
            author,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    let title_text = doc
        .select(&Selector::parse("title").unwrap())
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_else(|| "Musify".to_string());
    let desc = doc
        .select(&Selector::parse(r#"meta[property="og:description"]"#).unwrap())
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());
    let image = doc
        .select(&Selector::parse(r#"meta[property="og:image"]"#).unwrap())
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok(HubData {
        title: title_text,
        description: desc,
        link: Some(url),
        image,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_MUSIFY_LATEST: Route = Route {
    meta: &META_MUSIFY_LATEST,
    handler: handler_fn,
};
