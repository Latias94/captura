use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, Duration, FixedOffset, Utc};
use scraper::{ElementRef, Html, Selector};

pub const META_BANGUMI_SUBJECT_COMMENTS: RouteMeta = RouteMeta {
    hub_id: "bangumi.tv/subject_comments",
    path: "/bangumi.tv/subject/:id/comments",
    categories: &["anime"],
    example: "/bangumi.tv/subject/328609/comments",
    params: &[
        ParamMeta {
            name: "id",
            description: "Bangumi subject id, e.g. 328609.",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "min_length",
            description:
                "Minimum comment length to include (number of characters), default 0 (no filter).",
            default: Some("0"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &["bgm.tv/subject/:id", "bangumi.tv/subject/:id"],
        target: "/subject/:id/comments",
    }],
    name: "Bangumi 条目吐槽",
    maintainers: &["captura"],
    url: "https://bangumi.tv",
    description:
        "Bangumi.tv subject comments (吐槽箱) scraped from HTML, aligned with RSSHub /bangumi.tv/subject/:id/comments route.",
    default_view: Some("articles"),
};

fn inner_text(el: &ElementRef<'_>) -> String {
    el.text().collect::<Vec<_>>().join("").trim().to_string()
}

fn parse_comment_date(text: &str) -> Option<DateTime<FixedOffset>> {
    let s = text.trim().trim_start_matches('@').trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();
    if lower.contains("ago") {
        return parse_relative_ago(&lower);
    }
    let first = s.split_whitespace().next().unwrap_or("");
    util::parse_date(first)
}

fn parse_relative_ago(s: &str) -> Option<DateTime<FixedOffset>> {
    let mut seconds: i64 = 0;
    for token in s.split_whitespace() {
        let t = token.trim();
        if t.is_empty() || t == "ago" {
            continue;
        }
        let (num_part, unit_char) = t.split_at(t.len().saturating_sub(1));
        let n: i64 = match num_part.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        match unit_char {
            "h" => seconds += n * 3600,
            "m" => seconds += n * 60,
            "d" => seconds += n * 86400,
            "s" => seconds += n,
            _ => {}
        }
    }
    if seconds == 0 {
        return None;
    }
    let now = Utc::now();
    let ts = now - Duration::seconds(seconds);
    let offset = FixedOffset::east_opt(0)?;
    Some(DateTime::from_utc(ts.naive_utc(), offset))
}

fn extract_rating(class_attr: &str) -> Option<String> {
    for part in class_attr.split_whitespace() {
        if let Some(rest) = part.strip_prefix("stars") {
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    None
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx.param_str("id").ok_or_else(|| {
        Error::Config("bangumi.tv/subject_comments: missing subject id".to_string())
    })?;
    let min_length = ctx.param_i64("min_length").unwrap_or(0).max(0) as usize;
    let limit = ctx.param_i64("limit").unwrap_or(50).max(1) as usize;

    let link = format!("https://bgm.tv/subject/{}/comments", id);
    let html = util::get_html(&link)
        .await
        .map_err(|e| Error::Network(format!("bangumi.tv comments error: {}", e)))?;

    let doc = Html::parse_document(&html);
    let sel_title = Selector::parse("h1.nameSingle a")
        .map_err(|e| Error::Parse(format!("selector error: {e}")))?;
    let sel_item = Selector::parse("#comment_box .item.clearit")
        .map_err(|e| Error::Parse(format!("selector error: {e}")))?;
    let sel_user =
        Selector::parse("a.l").map_err(|e| Error::Parse(format!("selector error: {e}")))?;
    let sel_star =
        Selector::parse(".starlight").map_err(|e| Error::Parse(format!("selector error: {e}")))?;
    let sel_small =
        Selector::parse("small.grey").map_err(|e| Error::Parse(format!("selector error: {e}")))?;
    let sel_comment =
        Selector::parse("p.comment").map_err(|e| Error::Parse(format!("selector error: {e}")))?;

    let subject_title = doc
        .select(&sel_title)
        .next()
        .map(|a| inner_text(&a))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("Bangumi subject {}", id));

    let mut items = Vec::new();

    for el in doc.select(&sel_item).take(limit) {
        let user = el
            .select(&sel_user)
            .next()
            .map(|a| inner_text(&a))
            .unwrap_or_default();

        let mut small_iter = el.select(&sel_small);
        let _status = small_iter.next();
        let date_text = small_iter
            .next()
            .map(|s| inner_text(&s))
            .unwrap_or_default();

        let comment = el
            .select(&sel_comment)
            .next()
            .map(|p| inner_text(&p))
            .unwrap_or_default();

        if min_length > 0 && comment.chars().count() < min_length {
            continue;
        }

        let rate = el
            .select(&sel_star)
            .next()
            .and_then(|span| span.value().attr("class"))
            .and_then(extract_rating)
            .unwrap_or_else(|| "无".to_string());

        let description = format!("【评分：{}】  {}", rate, comment);
        let pub_date = parse_comment_date(&date_text);

        let title = if user.trim().is_empty() {
            "匿名用户的吐槽".to_string()
        } else {
            format!("{}的吐槽", user.trim())
        };

        items.push(HubItem {
            title,
            description: Some(description),
            link: Some(link.clone()),
            author: if user.trim().is_empty() {
                None
            } else {
                Some(user.trim().to_string())
            },
            pub_date,
            categories: vec![
                "Bangumi".to_string(),
                "Anime".to_string(),
                "Comments".to_string(),
            ],
        });
    }

    Ok(HubData {
        title: format!("{}的 Bangumi 吐槽箱", subject_title),
        description: Some("Bangumi 番组计划条目用户吐槽列表。".to_string()),
        link: Some(link),
        image: None,
        language: Some("zh-CN".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_BANGUMI_SUBJECT_COMMENTS: Route = Route {
    meta: &META_BANGUMI_SUBJECT_COMMENTS,
    handler: handler_fn,
};
