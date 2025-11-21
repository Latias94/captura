use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::FixedOffset;
use encoding_rs::GBK;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.jjwxc.net";

pub const META_JJWXC_BOOK: RouteMeta = RouteMeta {
    hub_id: "jjwxc/book",
    path: "/jjwxc/book/:id",
    categories: &["reading"],
    example: "/jjwxc/book/7013024",
    params: &[ParamMeta {
        name: "id",
        description: "Novel id, can be found in the JJWXC work URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.jjwxc.net"],
        target: "/book/:id",
    }],
    name: "晋江文学城 - 作品章节",
    maintainers: &["captura"],
    url: "https://www.jjwxc.net",
    description:
        "JJWXC novel chapter list, roughly aligned with RSSHub /jjwxc/book/:id route (simplified description rendering).",
    default_view: Some("notifications"),
};

fn parse_pub_date(s: &str) -> Option<chrono::DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

fn decode_gbk(bytes: &[u8]) -> Result<String, Error> {
    let (cow, _, had_errors) = GBK.decode(bytes);
    if had_errors {
        return Err(Error::Parse("jjwxc/book: GBK decode error".to_string()));
    }
    Ok(cow.into_owned())
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("jjwxc/book: id is required".to_string()))?;
    let limit = ctx.param_i64("limit").unwrap_or(100).max(1) as usize;

    let current_url = format!("{ROOT_URL}/onebook.php?novelid={}", id);

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("jjwxc/book client error: {}", e)))?;
    let resp = client
        .get(&current_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("jjwxc/book: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "jjwxc/book: http status {}",
            status
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let html = decode_gbk(&bytes)?;

    // Parse list page and basic metadata in a separate scope so that
    // the non-Send Html document does not live across await points.
    let (author, category, mut items_raw, image, description) = {
        let doc = Html::parse_document(&html);

        let sel_meta_author =
            Selector::parse(r#"meta[name="Author"]"#).map_err(|e| Error::Parse(e.to_string()))?;
        let sel_meta_keywords =
            Selector::parse(r#"meta[name="Keywords"]"#).map_err(|e| Error::Parse(e.to_string()))?;
        let sel_logo =
            Selector::parse("div.logo a img").map_err(|e| Error::Parse(e.to_string()))?;
        let sel_desc = Selector::parse(r#"span[itemprop="description"]"#)
            .map_err(|e| Error::Parse(e.to_string()))?;
        let sel_chapter = Selector::parse(r#"tr[itemprop="chapter"]"#)
            .map_err(|e| Error::Parse(e.to_string()))?;

        let author = doc
            .select(&sel_meta_author)
            .next()
            .and_then(|el| el.value().attr("content"))
            .unwrap_or("")
            .to_string();
        let mut keywords = doc
            .select(&sel_meta_keywords)
            .next()
            .and_then(|el| el.value().attr("content"))
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>();
        if !keywords.is_empty() {
            keywords.pop();
        }
        let category = keywords.pop();

        let mut items_raw = Vec::new();
        let sel_td = Selector::parse("td").unwrap();
        let sel_headline = Selector::parse(r#"span[itemprop="headline"]"#).unwrap();
        let sel_wordcount = Selector::parse(r#"td[itemprop="wordCount"]"#).unwrap();

        for row in doc.select(&sel_chapter) {
            let mut tds = row.select(&sel_td);
            let chapter_id = tds
                .next()
                .map(|td| td.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let headline_el = row.select(&sel_headline).next();
            let chapter_name = headline_el
                .as_ref()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let is_vip = headline_el
                .and_then(|el| el.select(&Selector::parse("font").unwrap()).last())
                .map(|font| font.text().collect::<String>())
                .map(|s| s.trim() == "[VIP]")
                .unwrap_or(false);

            let chapter_intro = row
                .select(&sel_td)
                .nth(2)
                .map(|td| td.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let chapter_words = row
                .select(&sel_wordcount)
                .next()
                .map(|td| td.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let chapter_clicks = row
                .select(&Selector::parse("td.chapterclick").unwrap())
                .next()
                .map(|td| td.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let updated_text = row
                .select(&sel_td)
                .last()
                .map(|td| td.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let is_lock = row
                .select(&sel_td)
                .nth(1)
                .map(|td| td.text().collect::<String>().trim().to_string() == "[锁]")
                .unwrap_or(false);

            if chapter_id.is_empty() || chapter_name.is_empty() {
                continue;
            }

            let chapter_url = format!(
                "{ROOT_URL}/onebook.php?novelid={}&chapterid={}",
                id, chapter_id
            );

            items_raw.push((
                chapter_id,
                chapter_name,
                chapter_intro,
                chapter_url,
                chapter_words,
                chapter_clicks,
                updated_text,
                is_vip,
                is_lock,
            ));
        }

        let logo_el = doc
            .select(&sel_logo)
            .next()
            .ok_or_else(|| Error::Parse("jjwxc/book: logo img not found".to_string()))?;
        let image = logo_el.value().attr("src").unwrap_or("");
        let image = if image.starts_with("http") {
            image.to_string()
        } else {
            format!("https:{}", image)
        };
        let description = doc
            .select(&sel_desc)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();

        (author, category, items_raw, image, description)
    };

    // reverse to get latest first, then limit
    items_raw.reverse();
    items_raw.truncate(limit);

    let mut items = Vec::new();

    for (
        chapter_id,
        chapter_name,
        chapter_intro,
        chapter_url,
        words,
        clicks,
        updated,
        is_vip,
        is_lock,
    ) in items_raw
    {
        let title = format!("{} {}", chapter_name, chapter_intro);
        let mut description = format!(
            "<p>章节ID: {} 字数: {} 点击: {} 更新时间: {}</p>",
            chapter_id, words, clicks, updated
        );
        let pub_date = parse_pub_date(&updated);

        if !is_vip && !is_lock {
            if let Ok(detail_resp) = client.get(&chapter_url).send().await {
                if detail_resp.status().is_success() {
                    if let Ok(detail_html) = detail_resp.text().await {
                        let detail_doc = Html::parse_document(&detail_html);
                        let sel_body = Selector::parse("div.novelbody")
                            .map_err(|e| Error::Parse(e.to_string()))?;
                        if let Some(body) = detail_doc.select(&sel_body).next() {
                            description.push_str(&body.html());
                        }
                    }
                }
            }
        }

        let mut categories = Vec::new();
        if is_vip {
            categories.push("VIP".to_string());
        }
        if let Some(cat) = &category {
            for part in cat.split_whitespace() {
                if !part.is_empty() {
                    categories.push(part.to_string());
                }
            }
        }

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(chapter_url),
            author: Some(author.clone()),
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: format!("晋江文学城 | {}", author),
        description: Some(description),
        link: Some(current_url),
        image: Some(image),
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_JJWXC_BOOK: Route = Route {
    meta: &META_JJWXC_BOOK,
    handler: handler_fn,
};
