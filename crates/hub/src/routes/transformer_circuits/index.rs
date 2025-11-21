use crate::routes::types::{Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://transformer-circuits.pub";

fn has_class(value: &scraper::node::Element, name: &str) -> bool {
    value
        .attr("class")
        .map(|c| c.split_whitespace().any(|t| t == name))
        .unwrap_or(false)
}

fn extract_list(
    html: &str,
    limit: usize,
) -> captura_common::Result<Vec<(String, String, Option<String>, String, String)>> {
    let doc = Html::parse_document(html);
    let sel_toc = Selector::parse(".toc")
        .map_err(|e| Error::Parse(format!("tcircuits: toc selector error: {e}")))?;
    let sel_a = Selector::parse("a")
        .map_err(|e| Error::Parse(format!("tcircuits: link selector error: {e}")))?;
    let sel_h3 = Selector::parse("h3")
        .map_err(|e| Error::Parse(format!("tcircuits: title selector error: {e}")))?;
    let sel_byline = Selector::parse(".byline")
        .map_err(|e| Error::Parse(format!("tcircuits: byline selector error: {e}")))?;
    let sel_desc = Selector::parse(".description")
        .map_err(|e| Error::Parse(format!("tcircuits: desc selector error: {e}")))?;
    let sel_date = Selector::parse(".date")
        .map_err(|e| Error::Parse(format!("tcircuits: date selector error: {e}")))?;

    let mut out = Vec::new();

    for toc in doc.select(&sel_toc) {
        let mut current_date: Option<String> = None;

        for child in toc.children() {
            let el = match scraper::ElementRef::wrap(child) {
                Some(e) => e,
                None => continue,
            };
            let value = el.value();
            if value.name() == "div" && has_class(value, "date") {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    current_date = Some(text);
                }
                continue;
            }
            if value.name() == "a" && (has_class(value, "paper") || has_class(value, "note")) {
                let href = value.attr("href").unwrap_or("").trim();
                if href.is_empty() {
                    continue;
                }
                let link = util::absolutize(ROOT_URL, href);

                let title = el
                    .select(&sel_h3)
                    .next()
                    .map(|h| h.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                if title.is_empty() {
                    continue;
                }

                let byline = el
                    .select(&sel_byline)
                    .next()
                    .map(|b| b.text().collect::<String>().trim().to_string())
                    .filter(|s| !s.is_empty());

                let desc_text = el
                    .select(&sel_desc)
                    .next()
                    .map(|d| d.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();

                let article_type = if has_class(value, "paper") {
                    "Paper"
                } else {
                    "Note"
                };

                let mut summary = String::new();
                summary.push_str(article_type);
                if !desc_text.is_empty() {
                    summary.push_str(": ");
                    summary.push_str(&desc_text);
                }

                let date_str = current_date.clone();

                out.push((title, link, byline, summary, date_str.unwrap_or_default()));

                if out.len() >= limit {
                    return Ok(out);
                }
            }
        }
    }

    Ok(out)
}

fn extract_article_body(html: &str, url: &str) -> String {
    let doc = Html::parse_document(html);

    if let Ok(sel) = Selector::parse("d-article") {
        if let Some(el) = doc.select(&sel).next() {
            let body = el.html();
            if !body.trim().is_empty() {
                return body;
            }
        }
    }

    let fallbacks = [
        "main article",
        ".article-content",
        ".post-content",
        ".content-area",
        ".content",
        ".article",
        ".post",
        "main",
    ];

    for css in &fallbacks {
        if let Ok(sel) = Selector::parse(css) {
            if let Some(el) = doc.select(&sel).next() {
                let body = el.html();
                if !body.trim().is_empty() {
                    return body;
                }
            }
        }
    }

    format!(
        "<p>Could not extract content. Please visit <a href=\"{url}\">the original page</a>.</p>"
    )
}

pub const META_TRANSFORMER_CIRCUITS: RouteMeta = RouteMeta {
    hub_id: "transformer-circuits",
    path: "/transformer-circuits",
    categories: &["programming"],
    example: "/transformer-circuits",
    params: &[],
    features: Features::basic(),
    radar: &[Radar {
        source: &["transformer-circuits.pub/"],
        target: "/",
    }],
    name: "Transformer Circuits Articles",
    maintainers: &["captura"],
    url: "https://transformer-circuits.pub",
    description:
        "Anthropic's Transformer Circuits thread: papers and notes on reverse engineering transformer language models.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let limit = ctx.param_i64("limit").unwrap_or(40).max(1) as usize;
    let url = ROOT_URL.to_string();
    let html = util::get_html(&url).await?;

    let list = extract_list(&html, limit)?;

    let mut items = Vec::new();

    for (title, link, byline, summary, _date_str) in list {
        let mut description = summary.clone();

        if let Ok(article_html) = util::get_html(&link).await {
            let body = extract_article_body(&article_html, &link);
            if !body.trim().is_empty() {
                description = body;
            }
        }

        let author = byline;

        let categories = vec![
            "AI".to_string(),
            "Machine Learning".to_string(),
            "Anthropic".to_string(),
            "Transformer Circuits".to_string(),
        ];

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link),
            author,
            pub_date: None,
            categories,
        });
    }

    Ok(HubData {
        title: "Transformer Circuits Thread".to_string(),
        description: Some(
            "Research on reverse engineering transformer language models into human-understandable programs."
                .to_string(),
        ),
        link: Some(url),
        image: None,
        language: Some("en-US".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_TRANSFORMER_CIRCUITS: Route = Route {
    meta: &META_TRANSFORMER_CIRCUITS,
    handler: handler_fn,
};
