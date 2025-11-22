use crate::routes::types::{Features, HubCtx, HubData, HubItem, ParamMeta, Route, RouteMeta};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use scraper::{Html, Selector};

pub const META_PS_TROPHY: RouteMeta = RouteMeta {
    hub_id: "ps/trophy",
    path: "/ps/trophy/:id",
    categories: &["game"],
    example: "/ps/trophy/DIYgod_",
    params: &[ParamMeta {
        name: "id",
        description: "PSN user id.",
        default: None,
        options: &[],
    }],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: true,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: false,
    },
    radar: &[],
    name: "PlayStation Network user trophy",
    maintainers: &["captura"],
    url: "https://psnprofiles.com",
    description: "Recent trophies of a PSN user.",
    default_view: Some("notifications"),
};

fn parse_pub_date(date_str: &str, time_str: &str) -> Option<DateTime<FixedOffset>> {
    let s = format!("{} {}", date_str.trim(), time_str.trim());
    let naive = NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M").ok()?;
    let offset = FixedOffset::east_opt(8 * 3600)?;
    Some(offset.from_local_datetime(&naive).single()?)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let id = ctx
        .param_str("id")
        .ok_or_else(|| Error::Config("ps/trophy: id is required".to_string()))?;

    let client = client_basic(None, None).map_err(|e| Error::Network(e.to_string()))?;

    let url = format!("https://psnprofiles.com/{}?order=last-trophy", id);
    let html = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::Network(format!("ps/trophy list -> {}", e)))?
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    let mut game_links = Vec::new();
    {
        let doc = Html::parse_document(&html);
        let row_sel = Selector::parse(".zebra tr").unwrap();
        let progress_sel = Selector::parse(".progress-bar span").unwrap();
        let title_sel = Selector::parse(".title").unwrap();

        for row in doc.select(&row_sel) {
            let progress = row
                .select(&progress_sel)
                .next()
                .map(|s| crate::routes::util::element_text(&s))
                .unwrap_or_default();
            if progress.trim() == "0%" {
                continue;
            }
            if let Some(a) = row.select(&title_sel).next() {
                if let Some(href) = a.value().attr("href") {
                    game_links.push(href.to_string());
                }
            }
            if game_links.len() >= 3 {
                break;
            }
        }
    }

    let mut all_items: Vec<HubItem> = Vec::new();

    for game in game_links.into_iter() {
        let link = format!(
            "https://psnprofiles.com{}?order=date&trophies=earned&lang=zh-cn",
            game
        );
        let html = client
            .get(&link)
            .send()
            .await
            .map_err(|e| Error::Network(format!("ps/trophy game -> {}", e)))?
            .text()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let doc = Html::parse_document(&html);

        let page_title = {
            let h_sel = Selector::parse(".page h3").unwrap();
            doc.select(&h_sel)
                .next()
                .map(|h| crate::routes::util::element_text(&h))
                .unwrap_or_default()
        };

        let row_sel = Selector::parse(".zebra tr.completed").unwrap();
        let cell_sel = Selector::parse("td").unwrap();
        let img_sel = Selector::parse(".trophy source").unwrap();
        let title_sel = Selector::parse(".title").unwrap();

        for row in doc.select(&row_sel) {
            let mut cells = row.select(&cell_sel);
            let level_cell = cells.nth(5);

            let trophy_title = row
                .select(&title_sel)
                .next()
                .map(|t| crate::routes::util::element_text(&t))
                .unwrap_or_default();
            if trophy_title.is_empty() {
                continue;
            }

            let a = row
                .select(&title_sel)
                .next()
                .and_then(|t| t.value().attr("href"))
                .map(|h| format!("https://psnprofiles.com{}", h));

            let img_url = row
                .select(&img_sel)
                .next()
                .and_then(|s| s.value().attr("srcset"))
                .and_then(|s| s.split_whitespace().last())
                .map(|s| s.to_string());

            let desc_text = row
                .select(&title_sel)
                .next()
                .map(|t| crate::routes::util::element_text(&t))
                .unwrap_or_default();

            let level = level_cell
                .and_then(|c| {
                    let img_sel = Selector::parse("img").unwrap();
                    c.select(&img_sel)
                        .next()
                        .and_then(|i| i.value().attr("title"))
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();

            let class_map = |s: &str| match s {
                "Platinum" => "白金".to_string(),
                "Gold" => "金".to_string(),
                "Silver" => "银".to_string(),
                "Bronze" => "铜".to_string(),
                _ => s.to_string(),
            };

            let date_text = {
                let sel = Selector::parse(".typo-top-date nobr").unwrap();
                row.select(&sel)
                    .next()
                    .map(|n| crate::routes::util::element_text(&n))
                    .unwrap_or_default()
            };
            let time_text = {
                let sel = Selector::parse(".typo-bottom-date").unwrap();
                row.select(&sel)
                    .next()
                    .map(|n| crate::routes::util::element_text(&n))
                    .unwrap_or_default()
            };
            let pub_date = parse_pub_date(&date_text, &time_text);

            let mut description = String::new();
            if let Some(ref img) = img_url {
                description.push_str(&format!(r#"<img src="{}"><br>"#, img));
            }
            if !desc_text.trim().is_empty() {
                description.push_str(desc_text.trim());
                description.push_str("<br>");
            }
            if !level.is_empty() {
                description.push_str("等级：");
                description.push_str(&class_map(&level));
            }

            all_items.push(HubItem {
                title: format!("{} - {}", trophy_title, page_title),
                description: if description.is_empty() {
                    None
                } else {
                    Some(description)
                },
                link: a,
                author: None,
                pub_date,
                categories: vec!["ps".to_string(), "trophy".to_string()],
            });
        }
    }

    all_items.sort_by(|a, b| b.pub_date.cmp(&a.pub_date));

    Ok(HubData {
        title: format!("{} 的 PSN 奖杯", id),
        description: None,
        link: Some(format!("https://psnprofiles.com/{}/log", id)),
        image: None,
        language: Some("zh-CN".to_string()),
        items: all_items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_PS_TROPHY: Route = Route {
    meta: &META_PS_TROPHY,
    handler: handler_fn,
};
