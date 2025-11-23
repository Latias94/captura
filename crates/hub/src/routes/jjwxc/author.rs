use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::FixedOffset;
use encoding_rs::GBK;
use scraper::{Html, Selector};

const ROOT_URL: &str = "https://www.jjwxc.net";

pub const META_JJWXC_AUTHOR: RouteMeta = RouteMeta {
    hub_id: "jjwxc/author",
    path: "/jjwxc/author/:id",
    categories: &["reading"],
    example: "/jjwxc/author/4364484",
    params: &[ParamMeta {
        name: "id",
        description: "Author id, can be found in the JJWXC author page URL.",
        default: None,
        options: &[],
    }],
    features: Features::basic(),
    radar: &[Radar {
        source: &["www.jjwxc.net"],
        target: "/author/:id",
    }],
    name: "晋江文学城 - 作者最新作品",
    maintainers: &["captura"],
    url: "https://www.jjwxc.net",
    description: "JJWXC author latest work summary, roughly aligned with RSSHub /jjwxc/author/:id route.",
    default_view: Some("articles"),
};

fn decode_gbk(bytes: &[u8]) -> Result<String, Error> {
    let (cow, _, had_errors) = GBK.decode(bytes);
    if had_errors {
        return Err(Error::Parse("jjwxc: GBK decode error".to_string()));
    }
    Ok(cow.into_owned())
}

fn parse_pub_date(s: &str) -> Option<chrono::DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("jjwxc/author: id is required".to_string()))?;

    let current_url = format!("{ROOT_URL}/oneauthor.php?authorid={}", id);
    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("jjwxc/author client error: {}", e)))?;
    let resp = client
        .get(&current_url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("jjwxc/author: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "jjwxc/author: http status {}",
            status
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;
    let html = decode_gbk(&bytes)?;
    let doc = Html::parse_document(&html);

    let sel_book_font = Selector::parse("font a").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_author_name =
        Selector::parse(r#"span[itemprop="name"]"#).map_err(|e| Error::Parse(e.to_string()))?;
    let sel_logo = Selector::parse("div.logo a img").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_desc = Selector::parse(r#"span[itemprop="description"]"#)
        .map_err(|e| Error::Parse(e.to_string()))?;
    let sel_meta_desc =
        Selector::parse(r#"meta[name="Description"]"#).map_err(|e| Error::Parse(e.to_string()))?;

    let book_el = doc
        .select(&sel_book_font)
        .next()
        .ok_or_else(|| Error::Parse("jjwxc/author: book element not found".to_string()))?;
    let book_name = book_el.text().collect::<String>().trim().to_string();
    let book_url = book_el
        .value()
        .attr("href")
        .map(|href| {
            if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{}{}", ROOT_URL, href)
            }
        })
        .unwrap_or_else(|| ROOT_URL.to_string());

    let book_info_parent = book_el
        .parent()
        .and_then(|node| scraper::ElementRef::wrap(node))
        .ok_or_else(|| Error::Parse("jjwxc/author: book info parent not found".to_string()))?;

    let mut book_info_fonts = book_info_parent
        .select(&Selector::parse("font").unwrap())
        .collect::<Vec<_>>();
    let book_status = book_info_fonts
        .get(0)
        .map(|f| f.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let book_words = book_info_fonts
        .get(1)
        .map(|f| f.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let book_updated_time = book_info_parent
        .parent()
        .and_then(|node| scraper::ElementRef::wrap(node))
        .map(|p| p.text().collect::<String>())
        .unwrap_or_default();
    let book_updated_time = book_updated_time.trim().to_string();

    let author = doc
        .select(&sel_author_name)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();

    let title = format!(
        "{}({}/{}/{})",
        book_name, book_status, book_words, book_updated_time
    );

    let pub_date = parse_pub_date(&book_updated_time);

    let description_body = format!(
        "<p>书名: {}</p><p>状态: {}</p><p>字数: {}</p><p>更新时间: {}</p>",
        book_name, book_status, book_words, book_updated_time
    );

    let items = vec![HubItem {
        title: title.clone(),
        description: Some(description_body),
        link: Some(book_url.clone()),
        author: Some(author.clone()),
        pub_date,
        categories: if book_status.is_empty() {
            Vec::new()
        } else {
            vec![book_status.clone()]
        },
    }];

    let logo_el = doc
        .select(&sel_logo)
        .next()
        .ok_or_else(|| Error::Parse("jjwxc/author: logo img not found".to_string()))?;
    let logo_src = logo_el.value().attr("src").unwrap_or("");
    let image = if logo_src.starts_with("http") {
        logo_src.to_string()
    } else {
        format!("https:{}", logo_src)
    };
    let icon = format!("{ROOT_URL}/favicon.ico");
    let description = doc
        .select(&sel_desc)
        .next()
        .map(|el| el.text().collect::<String>().trim().to_string())
        .unwrap_or_default();
    let subtitle = doc
        .select(&sel_meta_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    Ok(HubData {
        title: format!("晋江文学城 | {} - 最近更新", author),
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
pub const ROUTE_JJWXC_AUTHOR: Route = Route {
    meta: &META_JJWXC_AUTHOR,
    handler: handler_fn,
};
