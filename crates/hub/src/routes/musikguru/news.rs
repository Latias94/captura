use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset};
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://musikguru.de";

fn parse_pub_date(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub const META_MUSIKGURU_NEWS: RouteMeta = RouteMeta {
    hub_id: "musikguru/news",
    path: "/musikguru/news",
    categories: &["multimedia"],
    example: "/musikguru/news",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["musikguru.de/news"],
        target: "/news",
    }],
    name: "MusikGuru News",
    maintainers: &["captura"],
    url: "https://musikguru.de/news",
    description: "MusikGuru news posts, aligned with RSSHub /musikguru/news route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(10).max(1) as usize;
    let url = format!("{}/news/", ROOT_URL);

    let html = util::get_html(&url).await?;
    let (language, title, desc, image, cards) = {
        let doc = Html::parse_document(&html);

        let language = doc
            .select(&Selector::parse("html").unwrap())
            .next()
            .and_then(|el| el.value().attr("lang"))
            .unwrap_or("de")
            .to_string();

        let sel_section = Selector::parse("section")
            .map_err(|e| Error::Parse(format!("musikguru: section selector error: {e}")))?;
        let sel_card = Selector::parse("div.card")
            .map_err(|e| Error::Parse(format!("musikguru: card selector error: {e}")))?;
        let sel_title = Selector::parse("h5.card-title")
            .map_err(|e| Error::Parse(format!("musikguru: title selector error: {e}")))?;
        let sel_img = Selector::parse("img")
            .map_err(|e| Error::Parse(format!("musikguru: img selector error: {e}")))?;
        let sel_intro = Selector::parse("p.card-text")
            .map_err(|e| Error::Parse(format!("musikguru: intro selector error: {e}")))?;
        let sel_link = Selector::parse("a")
            .map_err(|e| Error::Parse(format!("musikguru: link selector error: {e}")))?;

        let mut cards = Vec::new();

        if let Some(section) = doc.select(&sel_section).nth(1) {
            for card in section.select(&sel_card).take(limit) {
                let title = card
                    .select(&sel_title)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                if title.is_empty() {
                    continue;
                }

                let image = card
                    .select(&sel_img)
                    .next()
                    .and_then(|el| el.value().attr("src"))
                    .map(|s| util::absolutize(ROOT_URL, s));

                let intro = card
                    .select(&sel_intro)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let link = card
                    .select(&sel_link)
                    .next()
                    .and_then(|el| el.value().attr("href"))
                    .map(|href| util::absolutize(ROOT_URL, href));

                cards.push((title, image, intro, link));
            }
        }

        let title = doc
            .select(&Selector::parse("title").unwrap())
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "MusikGuru".to_string());
        let desc = doc
            .select(&Selector::parse(r#"meta[name="description"]"#).unwrap())
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string());
        let image = doc
            .select(&Selector::parse("a.navbar-brand img").unwrap())
            .next()
            .and_then(|el| el.value().attr("src"))
            .map(|s| util::absolutize(ROOT_URL, s));

        (language, title, desc, image, cards)
    };

    let mut items = Vec::new();

    for (title, image, intro, link) in cards {
        let mut description = String::new();
        if let Some(ref img) = image {
            description.push_str(&format!(
                "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                src = img,
                alt = title
            ));
        }
        if !intro.is_empty() {
            description.push_str(&format!("<p>{}</p>", intro));
        }

        let mut pub_date = None;
        if let Some(ref article_link) = link {
            if let Ok(detail_html) = util::get_html(article_link).await {
                let doc_detail = Html::parse_document(&detail_html);
                let sel_article_title = Selector::parse("div.article h1").map_err(|e| {
                    Error::Parse(format!("musikguru: article title selector error: {e}"))
                })?;
                let sel_meta_text = Selector::parse("div.article div.text-muted").map_err(|e| {
                    Error::Parse(format!("musikguru: article meta selector error: {e}"))
                })?;
                let sel_lead_p = Selector::parse("p.lead")
                    .map_err(|e| Error::Parse(format!("musikguru: lead p selector error: {e}")))?;
                let sel_lead_div = Selector::parse("div.lead").map_err(|e| {
                    Error::Parse(format!("musikguru: lead div selector error: {e}"))
                })?;
                let sel_img_detail = Selector::parse("div.article img").map_err(|e| {
                    Error::Parse(format!("musikguru: article img selector error: {e}"))
                })?;

                let full_title = doc_detail
                    .select(&sel_article_title)
                    .next()
                    .map(|el| el.text().collect::<String>().trim().to_string())
                    .unwrap_or(title.clone());

                let mut detail_desc = String::new();
                if let Some(img) = doc_detail
                    .select(&sel_img_detail)
                    .next()
                    .and_then(|el| el.value().attr("src"))
                {
                    detail_desc.push_str(&format!(
                        "<p><img src=\"{src}\" alt=\"{alt}\"></p>",
                        src = util::absolutize(ROOT_URL, img),
                        alt = full_title
                    ));
                }
                if let Some(lead) = doc_detail.select(&sel_lead_p).next().map(|el| el.html()) {
                    detail_desc.push_str(&lead);
                }
                if let Some(lead_div) = doc_detail.select(&sel_lead_div).next().map(|el| el.html())
                {
                    detail_desc.push_str(&lead_div);
                }

                if !detail_desc.is_empty() {
                    description.push_str(&detail_desc);
                }

                if let Some(meta_text) = doc_detail
                    .select(&sel_meta_text)
                    .next()
                    .map(|el| el.text().collect::<String>())
                {
                    let text = meta_text.split(" Uhr").next().unwrap_or("").trim();
                    if !text.is_empty() {
                        pub_date = parse_pub_date(text)
                            .or_else(|| parse_pub_date(&format!("{text} 00:00")));
                    }
                }

                items.push(HubItem {
                    title: full_title,
                    description: Some(description.clone()),
                    link: link.clone(),
                    author: None,
                    pub_date,
                    categories: Vec::new(),
                });
                continue;
            }
        }

        items.push(HubItem {
            title,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            link,
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    Ok(HubData {
        title,
        description: desc,
        link: Some(url),
        image,
        language: Some(language),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_MUSIKGURU_NEWS: Route = Route {
    meta: &META_MUSIKGURU_NEWS,
    handler: handler_fn,
};
