use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use scraper::{Html, Selector};
use std::collections::HashMap;

const HOST: &str = "https://www.dlsite.com";

#[derive(Clone, Debug)]
struct DlsiteCampaignInfo {
    r#type: &'static str,
    name: &'static str,
    url: &'static str,
    params: HashMap<&'static str, DlsiteParam>,
}

#[derive(Clone, Debug)]
enum DlsiteParam {
    Single(&'static str),
    Multi(&'static [&'static str]),
}

fn build_params() -> Vec<DlsiteCampaignInfo> {
    vec![
        DlsiteCampaignInfo {
            r#type: "home",
            name: "「DLsite 同人」",
            url: "/home/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["doujin"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
        DlsiteCampaignInfo {
            r#type: "comic",
            name: "「DLsite コミック」",
            url: "/comic/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["books"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
        DlsiteCampaignInfo {
            r#type: "soft",
            name: "「DLsite PCソフト」",
            url: "/soft/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["pc"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
        DlsiteCampaignInfo {
            r#type: "maniax",
            name: "「DLsite 同人 - R18」",
            url: "/maniax/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["doujin"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
        DlsiteCampaignInfo {
            r#type: "books",
            name: "「DLsite 成年コミック - R18」",
            url: "/books/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["books"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
        DlsiteCampaignInfo {
            r#type: "pro",
            name: "「DLsite 美少女ゲーム」",
            url: "/pro/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["pc"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
        DlsiteCampaignInfo {
            r#type: "girls",
            name: "「DLsite 乙女」",
            url: "/girls/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["doujin"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
        DlsiteCampaignInfo {
            r#type: "bl",
            name: "「DLsite BL」",
            url: "/bl/fsr",
            params: {
                let mut m = HashMap::new();
                m.insert("campaign", DlsiteParam::Single("campaign"));
                m.insert("work_category", DlsiteParam::Multi(&["doujin"]));
                m.insert("order", DlsiteParam::Multi(&["cstart_d"]));
                m.insert("per_page", DlsiteParam::Single("30"));
                m.insert("show_type", DlsiteParam::Single("1"));
                m
            },
        },
    ]
}

fn find_info<'a>(infos: &'a [DlsiteCampaignInfo], t: &str) -> Option<&'a DlsiteCampaignInfo> {
    infos.iter().find(|info| info.r#type == t)
}

fn build_path(info: &DlsiteCampaignInfo, free: bool) -> String {
    let mut path = String::new();
    path.push_str(info.url.trim_start_matches('/'));
    path.push_str("/=/");

    for (name, param) in &info.params {
        match param {
            DlsiteParam::Single(v) => {
                path.push_str(name);
                path.push('/');
                path.push_str(v);
                path.push('/');
            }
            DlsiteParam::Multi(values) => {
                for (idx, v) in values.iter().enumerate() {
                    path.push_str(name);
                    path.push('[');
                    path.push_str(&idx.to_string());
                    path.push_str("]/");
                    path.push_str(v);
                    path.push('/');
                }
            }
        }
    }

    if free {
        path.push_str("is_free/1/");
    }

    path
}

pub const META_DLSITE_CAMPAIGN: RouteMeta = RouteMeta {
    hub_id: "dlsite/campaign",
    path: "/dlsite/campaign/:type/:free?",
    categories: &["anime"],
    example: "/dlsite/campaign/home",
    params: &[
        ParamMeta {
            name: "type",
            description: "DLsite area type. One of: home, comic, soft, maniax, books, pro, girls, bl.",
            default: Some("home"),
            options: &[
                ("home", "DLsite Doujin"),
                ("comic", "DLsite Comic"),
                ("soft", "DLsite PC Soft"),
                ("maniax", "DLsite Doujin R18"),
                ("books", "DLsite Adult Comic R18"),
                ("pro", "DLsite Bishoujo Games"),
                ("girls", "DLsite Otome"),
                ("bl", "DLsite BL"),
            ],
        },
        ParamMeta {
            name: "free",
            description: "Whether to filter free items (1 for free-only).",
            default: None,
            options: &[("1", "Only free works")],
        },
    ],
    features: Features {
        require_config: &[],
        require_puppeteer: false,
        anti_crawler: false,
        support_bt: false,
        support_podcast: false,
        support_scihub: false,
        nsfw: true,
    },
    radar: &[Radar {
        source: &["www.dlsite.com"],
        target: "/campaign/:type/:free?",
    }],
    name: "DLsite - Discounted Works",
    maintainers: &["captura"],
    url: "https://www.dlsite.com",
    description: "DLsite discounted works (campaign) list, including optional free-only filter, aligned with RSSHub /dlsite/campaign/:type/:free? route.",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let infos = build_params();
    let t = ctx.param_str("type").unwrap_or("home");
    let free_flag = ctx.param_str("free").is_some();

    let info = find_info(&infos, t).ok_or_else(|| {
        Error::Config(format!(
            "dlsite/campaign: unsupported type `{}`. Use one of home, comic, soft, maniax, books, pro, girls, bl.",
            t
        ))
    })?;

    let path = build_path(info, free_flag);
    let url = format!("{}/{}", HOST, path);

    let html = util::get_html(&url).await?;
    let doc = Html::parse_document(&html);

    let sel_desc =
        Selector::parse(r#"meta[name="description"]"#).map_err(|e| Error::Parse(e.to_string()))?;
    let description = doc
        .select(&sel_desc)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.to_string());

    let sel_list =
        Selector::parse(".n_worklist tr[class]").map_err(|e| Error::Parse(e.to_string()))?;
    let sel_name = Selector::parse(".work_name").unwrap();
    let sel_tags = Selector::parse(".search_tag a").unwrap();
    let sel_maker = Selector::parse(".maker_name").unwrap();

    let mut items = Vec::new();

    for el in doc.select(&sel_list) {
        let name_el = el.select(&sel_name).next();
        let a = match name_el.and_then(|n| n.select(&Selector::parse("a").unwrap()).next()) {
            Some(a) => a,
            None => continue,
        };
        let title = a.text().collect::<String>().trim().to_string();
        if title.is_empty() {
            continue;
        }
        let href = a.value().attr("href").unwrap_or("").trim();
        if href.is_empty() {
            continue;
        }

        let mut description_html = el.html();

        let mut categories = Vec::new();
        for tag in el.select(&sel_tags) {
            let text = tag.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                categories.push(text);
            }
        }
        let author = el
            .select(&sel_maker)
            .next()
            .map(|m| m.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty());

        items.push(HubItem {
            title,
            description: if description_html.trim().is_empty() {
                None
            } else {
                Some(description_html)
            },
            link: Some(util::absolutize(HOST, href)),
            author,
            pub_date: None,
            categories,
        });
    }

    Ok(HubData {
        title: format!("{} | 割引中の作品", info.name),
        description,
        link: Some(url),
        image: None,
        language: Some("ja-JP".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_DLSITE_CAMPAIGN: Route = Route {
    meta: &META_DLSITE_CAMPAIGN,
    handler: handler_fn,
};
