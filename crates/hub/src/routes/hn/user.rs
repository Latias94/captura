use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::{Error, Result};
use captura_hub_macros::register_hub_route;

pub const META_HACKERNEWS: RouteMeta = RouteMeta {
    hub_id: "hackernews",
    path: "/hackernews/:section?/:type?/:user?",
    categories: &["programming"],
    example: "/hackernews/threads/comments_list/dang",
    params: &[
        ParamMeta {
            name: "section",
            description:
                "Content section: index, newest, show, ask, over, threads, submitted (default: index)",
            default: Some("index"),
            options: &[
                ("index", "Front page (news)"),
                ("newest", "Newest stories"),
                ("show", "Show HN"),
                ("ask", "Ask HN"),
                ("over", "Over 100 points (or custom threshold)"),
                ("threads", "User threads (comments)"),
                ("submitted", "User submissions"),
            ],
        },
        ParamMeta {
            name: "type",
            description: "Link type: sources / comments / comments_list (default: sources)",
            default: Some("sources"),
            options: &[
                ("sources", "External source URL"),
                ("comments", "HN comments page"),
                ("comments_list", "Focus on user's comment text"),
            ],
        },
        ParamMeta {
            name: "user",
            description:
                "User id for threads/submitted sections; for section=over, interpreted as points threshold",
            default: None,
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "news.ycombinator.com/:section",
            "news.ycombinator.com",
        ],
        target: "/:section",
    }],
    name: "Hacker News (sections/users)",
    maintainers: &["captura"],
    url: "https://news.ycombinator.com/",
    description:
        "Hacker News section and user feeds (simplified version of RSSHub hackernews route).",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let section = ctx.param_str("section").unwrap_or("index");
    let link_type = ctx.param_str("type").unwrap_or("sources");
    let user = ctx.param_str("user").unwrap_or("");
    let limit = ctx.param_i64("limit").unwrap_or(30).max(1) as usize;

    let root = "https://news.ycombinator.com";
    let section_url = if section == "index" {
        String::new()
    } else {
        format!("/{}", section)
    };

    let mut opt = String::new();
    if section == "over" {
        if user.is_empty() {
            opt = "?points=100".to_string();
        } else {
            opt = format!("?points={}", user);
        }
    } else if (section == "threads" || section == "submitted") && !user.is_empty() {
        opt = format!("?id={}", user);
    } else if section == "threads" || section == "submitted" {
        return Err(Error::Config(
            "user is required when section is threads or submitted".into(),
        ));
    }

    let current_url = format!("{}{}{}", root, section_url, opt);
    let html = util::get_html(&current_url).await?;

    let items = if section == "threads" {
        collect_thread_items(&html, root, link_type, limit)?
    } else {
        collect_story_items(&html, root, link_type, limit)?
    };

    Ok(HubData {
        title: format!("Hacker News - {}", section),
        description: Some(format!(
            "Hacker News section={}, type={}, user={}",
            section,
            link_type,
            if user.is_empty() { "<none>" } else { user }
        )),
        link: Some(current_url),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn collect_story_items(
    html: &str,
    root: &str,
    link_type: &str,
    limit: usize,
) -> Result<Vec<HubItem>> {
    let mut items = Vec::new();

    util::for_each_element(html, "tr.athing", |el| {
        if items.len() >= limit {
            return;
        }

        let id = el.value().attr("id").map(|s| s.to_string());
        let origin = util::extract_attr(&el, "span.titleline a@href")
            .map(|href| util::absolutize(root, &href));
        let title = util::extract_text(&el, "span.titleline a");

        let (id, origin, title) = match (id, origin, title) {
            (Some(i), Some(o), Some(t)) => (i, o, t),
            _ => return,
        };

        let comments_link = format!("{}/item?id={}", root, id);
        let link = if link_type == "comments" || link_type == "comments_list" {
            comments_link.clone()
        } else {
            origin.clone()
        };

        let desc_html = format!(
            r#"<a href="{comments}">Comments on Hacker News</a> | <a href="{origin}">Source</a>"#,
            comments = comments_link,
            origin = origin,
        );

        items.push(HubItem {
            title,
            description: Some(desc_html),
            link: Some(link),
            author: None,
            pub_date: None,
            categories: Vec::new(),
        });
    })?;

    Ok(items)
}

fn collect_thread_items(
    html: &str,
    root: &str,
    link_type: &str,
    limit: usize,
) -> Result<Vec<HubItem>> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let sel = Selector::parse("tr.athing.comtr")
        .map_err(|e| Error::Parse(format!("invalid HN threads selector: {e}")))?;

    let sel_comment = Selector::parse("div.comment > div.commtext")
        .map_err(|e| Error::Parse(format!("invalid HN comment selector: {e}")))?;
    let sel_onstory = Selector::parse("span.onstory a")
        .map_err(|e| Error::Parse(format!("invalid HN onstory selector: {e}")))?;
    let sel_author = Selector::parse("a.hnuser")
        .map_err(|e| Error::Parse(format!("invalid HN author selector: {e}")))?;
    let sel_age = Selector::parse("span.age")
        .map_err(|e| Error::Parse(format!("invalid HN age selector: {e}")))?;

    let mut items = Vec::new();

    for el in doc.select(&sel).take(limit) {
        let id = el.value().attr("id").map(|s| s.to_string());
        let id = match id {
            Some(v) => v,
            None => continue,
        };

        let comment_html = el
            .select(&sel_comment)
            .next()
            .map(|node| util::element_html(&node))
            .unwrap_or_default();

        let on_story_title = el
            .select(&sel_onstory)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string());
        let on_story_href = el
            .select(&sel_onstory)
            .next()
            .and_then(|node| node.value().attr("href"))
            .map(|s| s.to_string());

        let author = el
            .select(&sel_author)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string());
        let age = el
            .select(&sel_age)
            .next()
            .and_then(|node| node.value().attr("title"))
            .map(|s| s.to_string());

        let comment_link = format!("{}/item?id={}", root, id);
        let story_link = on_story_href
            .as_ref()
            .map(|href| util::absolutize(root, href))
            .unwrap_or_else(|| comment_link.clone());

        let (title, link) = match link_type {
            "comments_list" => (
                on_story_title
                    .clone()
                    .unwrap_or_else(|| format!("Comment {}", id)),
                comment_link.clone(),
            ),
            "comments" => (
                on_story_title
                    .clone()
                    .unwrap_or_else(|| format!("Comment {}", id)),
                comment_link.clone(),
            ),
            _ => (
                on_story_title
                    .clone()
                    .unwrap_or_else(|| format!("Comment {}", id)),
                story_link.clone(),
            ),
        };

        let mut desc = String::new();
        if let Some(a) = &author {
            desc.push_str(&format!(
                r#"<div><small><a href="{root}/user?id={user}">{user}</a></small>"#,
                root = root,
                user = a,
            ));
            if let Some(age_str) = &age {
                desc.push_str(&format!(
                    r#" &nbsp;&nbsp;<small><a href="{link}">{age}</a></small>"#,
                    link = comment_link,
                    age = age_str
                ));
            }
            desc.push_str("</div>");
        }
        desc.push_str(&format!(r#"<div>{}</div>"#, comment_html));

        items.push(HubItem {
            title,
            description: Some(desc),
            link: Some(link),
            author,
            pub_date: age.and_then(|s| util::parse_date(&s)),
            categories: Vec::new(),
        });
    }

    Ok(items)
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_HACKERNEWS: Route = Route {
    meta: &META_HACKERNEWS,
    handler: handler_fn,
};
