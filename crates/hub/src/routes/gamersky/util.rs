use crate::routes::types::HubItem;
use crate::routes::util;
use captura_common::Result;
use scraper::{ElementRef, Html, Selector};

const BASE_URL: &str = "https://www.gamersky.com";

pub async fn get_article_list(node_id: &str) -> Result<String> {
    let json = serde_json::json!({
        "type": "updatenodelabel",
        "isCache": true,
        "cacheTime": 60,
        "nodeId": node_id,
        "isNodeId": "true",
        "page": 1,
    });

    let query = vec![(
        "jsondata".to_string(),
        serde_json::to_string(&json).unwrap_or_default(),
    )];
    let url = format!(
        "https://db2.gamersky.com/LabelJsonpAjax.aspx?{}",
        serde_urlencoded::to_string(&query).unwrap_or_default()
    );

    let raw = util::get_html(&url).await?;
    // Response is JSONP: callback({...});
    let body = raw
        .split_once('(')
        .and_then(|(_, rest)| rest.rsplit_once(')').map(|(inner, _)| inner.to_string()))
        .unwrap_or_else(|| raw.clone());

    #[derive(serde::Deserialize)]
    struct ArticleList {
        #[allow(dead_code)]
        status: String,
        #[allow(dead_code)]
        totalPages: i32,
        body: String,
    }

    let parsed: ArticleList = serde_json::from_str(&body)
        .map_err(|e| captura_common::Error::Parse(format!("gamersky list json -> {}", e)))?;
    Ok(parsed.body)
}

pub fn parse_article_list(html: &str) -> Vec<HubItem> {
    let doc = Html::parse_fragment(html);
    let li_sel = Selector::parse("li").unwrap();
    let time_sel = Selector::parse(".time").unwrap();
    let title_sel = Selector::parse(".tt").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let desc_sel = Selector::parse(".txt").unwrap();

    let mut out = Vec::new();

    fn element_text(el: &ElementRef<'_>) -> String {
        crate::routes::util::element_text(el)
    }

    for li in doc.select(&li_sel) {
        let link_el = if let Some(tt) = li.select(&title_sel).next() {
            tt
        } else if let Some(a) = li.select(&a_sel).next() {
            a
        } else {
            continue;
        };
        let href = match link_el.value().attr("href") {
            Some(h) => h,
            None => continue,
        };
        let link = util::absolutize(BASE_URL, href);
        let title = element_text(&link_el);
        if title.is_empty() {
            continue;
        }
        let description = li
            .select(&desc_sel)
            .next()
            .map(|d| element_text(&d))
            .filter(|s| !s.is_empty());
        let pub_date = li.select(&time_sel).next().and_then(|t| {
            let s = element_text(&t);
            util::parse_date(&s)
        });

        out.push(HubItem {
            title,
            description,
            link: Some(link),
            author: None,
            pub_date,
            categories: vec!["gamersky".to_string()],
        });
    }

    out
}

async fn get_article(item: &mut HubItem) -> Result<()> {
    let Some(ref link) = item.link else {
        return Ok(());
    };
    let html = util::get_html(link).await?;
    let doc = Html::parse_document(&html);
    let sel = Selector::parse(".Mid2L_con, .MidLcon").unwrap();
    if let Some(content) = doc.select(&sel).next() {
        let html_fragment = content.html();
        if !html_fragment.trim().is_empty() {
            item.description = Some(html_fragment);
        }
    }
    Ok(())
}

pub async fn enrich_items(mut items: Vec<HubItem>) -> Vec<HubItem> {
    let mut out = Vec::new();
    for mut item in items.drain(..) {
        if let Err(e) = get_article(&mut item).await {
            tracing::debug!("gamersky: get_article failed for {:?}: {}", item.link, e);
        }
        out.push(item);
    }
    out
}
