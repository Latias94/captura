use captura_common::{NormalizedEntry, Result};
use captura_rules::hub::types::{
    HandlerCtx as HubHandlerCtx, HubData, HubHandler, HubItem, HubResult,
};
use captura_rules::v1::RuleSpecV1;
use captura_storage::entity::feed;
use tracing::debug;

use crate::hub_utils;
use crate::rules_engine::{execute_json_v1, merge_rule_params_v1};

struct BuiltinHubRoute {
    hub_id: &'static str,
    handler: &'static dyn HubHandler,
}

static BUILTIN_HUB_ROUTES: &[BuiltinHubRoute] = &[
    BuiltinHubRoute {
        hub_id: "github/trending",
        handler: &GITHUB_TRENDING_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "hn/front",
        handler: &HN_FRONT_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "lobsters/front",
        handler: &LOBSTERS_FRONT_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/hot-search",
        handler: &BILIBILI_HOT_SEARCH_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/popular",
        handler: &BILIBILI_POPULAR_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/link/news",
        handler: &BILIBILI_LINK_NEWS_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/ranking",
        handler: &BILIBILI_RANKING_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/user/video",
        handler: &BILIBILI_USER_VIDEO_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/user/dynamic",
        handler: &BILIBILI_USER_DYNAMIC_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/bangumi/season",
        handler: &BILIBILI_BANGUMI_SEASON_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "bilibili/bangumi/media",
        handler: &BILIBILI_BANGUMI_MEDIA_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "zhihu/hotlist",
        handler: &ZHIHU_HOTLIST_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "reuters/top",
        handler: &REUTERS_TOP_HANDLER,
    },
    BuiltinHubRoute {
        hub_id: "medium/tag",
        handler: &MEDIUM_TAG_HANDLER,
    },
];

fn find_builtin_handler(hub_id: &str) -> Option<&'static dyn HubHandler> {
    BUILTIN_HUB_ROUTES
        .iter()
        .find(|r| r.hub_id == hub_id)
        .map(|r| r.handler)
}

/// Built-in Hub handler for github/trending.
pub struct GithubTrendingHubHandler;

#[async_trait::async_trait]
impl HubHandler for GithubTrendingHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let since = ctx.param_str("since").unwrap_or("daily");
        let language = ctx.param_str("language").unwrap_or("");
        let spoken = ctx.param_str("spoken_language").unwrap_or("");

        let mut url = if language.is_empty() || language == "any" {
            "https://github.com/trending".to_string()
        } else {
            format!("https://github.com/trending/{}", language)
        };
        let mut qs = vec![format!("since={}", since)];
        if !spoken.is_empty() {
            qs.push(format!("spoken_language_code={}", spoken));
        }
        if !qs.is_empty() {
            url.push('?');
            url.push_str(&qs.join("&"));
        }

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "article.Box-row", |el| {
            let link = crate::extract_attr(&el, "h2 a@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h2 a");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "GitHub Trending".to_string(),
            description: Some("GitHub trending repositories".to_string()),
            link: Some("https://github.com/trending".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "github_trending hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static GITHUB_TRENDING_HANDLER: GithubTrendingHubHandler = GithubTrendingHubHandler;

/// Built-in Hub handler for hn/front (Hacker News front page).
pub struct HnFrontHubHandler;

#[async_trait::async_trait]
impl HubHandler for HnFrontHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://news.ycombinator.com/".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "tr.athing", |el| {
            let link = crate::extract_attr(&el, "span.titleline a@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "span.titleline a");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title
                    .clone()
                    .unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Hacker News Front Page".to_string(),
            description: Some("Hacker News front page stories.".to_string()),
            link: Some("https://news.ycombinator.com/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "hn_front hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static HN_FRONT_HANDLER: HnFrontHubHandler = HnFrontHubHandler;

/// Built-in Hub handler for lobsters/front.
pub struct LobstersFrontHubHandler;

#[async_trait::async_trait]
impl HubHandler for LobstersFrontHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://lobste.rs/".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "li.story", |el| {
            let link = crate::extract_attr(&el, "h2 a@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h2 a");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Lobsters Front Page".to_string(),
            description: Some("Lobsters front page stories.".to_string()),
            link: Some("https://lobste.rs/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "lobsters_front hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static LOBSTERS_FRONT_HANDLER: LobstersFrontHubHandler = LobstersFrontHubHandler;

/// Built-in Hub handler for bilibili/hot-search.
pub struct BilibiliHotSearchHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliHotSearchHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let limit = ctx.param_str("limit").unwrap_or("10");
        let platform = ctx.param_str("platform").unwrap_or("web");

        let mut spec = captura_rules::bilibili::bilibili_hot_search_rule();
        // Override defaults with Hub-level arguments.
        if let Some(params) = &mut spec.params {
            params
                .defaults
                .insert("limit".to_string(), serde_json::json!(limit));
            params
                .defaults
                .insert("platform".to_string(), serde_json::json!(platform));
        }

        let feed_model = build_virtual_feed_for_spec(&spec);
        let entries = execute_json_v1(&feed_model, &spec).await?;

        let mut items: Vec<HubItem> = Vec::new();
        for e in entries {
            let title = e.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let keyword = title.clone();
            let icon = e.content_html.unwrap_or_default();

            let link = e
                .url
                .clone()
                .unwrap_or_else(|| build_bilibili_search_link(&keyword));

            let mut desc = keyword.clone();
            desc.push_str("<br>");
            if !icon.is_empty() {
                desc.push_str(&format!("<img src=\"{}\">", icon));
            }

            items.push(HubItem {
                title,
                description: Some(desc),
                link: Some(link),
                author: None,
                pub_date: None,
                categories: vec!["bilibili".to_string(), "hot-search".to_string()],
            });
        }

        let data = HubData {
            title: "bilibili 热搜".to_string(),
            description: Some("bilibili 热搜".to_string()),
            link: Some("https://www.bilibili.com".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_hot_search hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_HOT_SEARCH_HANDLER: BilibiliHotSearchHubHandler = BilibiliHotSearchHubHandler;

/// Built-in Hub handler for bilibili/popular.
pub struct BilibiliPopularHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliPopularHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        // Missing `embed` parameter means "embed enabled", matching RSSHub semantics.
        let embed = ctx.param_str("embed").is_none();

        let spec = captura_rules::bilibili::bilibili_popular_rule();
        let feed_model = build_virtual_feed_for_spec(&spec);
        let entries = execute_json_v1(&feed_model, &spec).await?;

        let mut items = Vec::new();
        for e in entries {
            let title = e.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            let summary = e.summary.unwrap_or_default();
            let cover = e.content_html.as_deref();
            let bvid = e.url.as_deref();
            let link = e
                .url
                .clone()
                .map(|b| format!("https://www.bilibili.com/video/{}", b))
                .unwrap_or_else(|| "https://www.bilibili.com".to_string());

            let description_html = captura_rules::bilibili::utils::render_ugc_description(
                embed, cover, &summary, bvid, None,
            );

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: e.author,
                pub_date: None,
                categories: vec!["bilibili".to_string(), "popular".to_string()],
            });
        }

        let data = HubData {
            title: "bilibili 综合热门".to_string(),
            description: Some("bilibili 综合热门".to_string()),
            link: Some("https://www.bilibili.com".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_popular hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_POPULAR_HANDLER: BilibiliPopularHubHandler = BilibiliPopularHubHandler;

/// Built-in Hub handler for bilibili/link/news.
pub struct BilibiliLinkNewsHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliLinkNewsHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let product = ctx.param_str("product").unwrap_or("live");

        let mut spec = captura_rules::bilibili::bilibili_link_news_rule();
        if let Some(params) = &mut spec.params {
            params
                .defaults
                .insert("product".to_string(), serde_json::json!(product));
        }

        let feed_model = build_virtual_feed_for_spec(&spec);
        let entries = execute_json_v1(&feed_model, &spec).await?;

        let product_title = match product {
            "vc" => "小视频",
            "wh" => "相簿",
            _ => "直播",
        };

        let mut items = Vec::new();
        for e in entries {
            let title = e.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            let description_html = e.content_html.unwrap_or_default();
            let link = e.url.clone().unwrap_or_else(|| {
                format!(
                    "https://link.bilibili.com/p/eden/news#/?tab={}&tag=all&page_no=1",
                    product
                )
            });

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: None,
                pub_date: None,
                categories: vec!["bilibili".to_string(), "link-news".to_string()],
            });
        }

        let data = HubData {
            title: format!("bilibili {}公告", product_title),
            link: Some(format!(
                "https://link.bilibili.com/p/eden/news#/?tab={}&tag=all&page_no=1",
                product
            )),
            description: Some(format!("bilibili {}公告", product_title)),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_link_news hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_LINK_NEWS_HANDLER: BilibiliLinkNewsHubHandler = BilibiliLinkNewsHubHandler;

/// Built-in Hub handler for bilibili/ranking.
pub struct BilibiliRankingHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliRankingHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let rid_param = ctx.param_str("rid").unwrap_or("0");
        let rid_numeric = if rid_param.is_empty() || rid_param == "all" {
            "0".to_string()
        } else {
            rid_param.to_string()
        };

        let mut spec = captura_rules::bilibili::bilibili_ranking_rule();
        if let Some(params) = &mut spec.params {
            params
                .defaults
                .insert("rid".to_string(), serde_json::json!(rid_numeric));
        }

        let feed_model = build_virtual_feed_for_spec(&spec);
        let entries = execute_json_v1(&feed_model, &spec).await?;

        let mut items = Vec::new();
        for e in entries {
            let title = e.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let summary = e.summary.unwrap_or_default();
            let cover = e.content_html.as_deref();
            let bvid = e.url.as_deref().unwrap_or_default();
            let link = if bvid.is_empty() {
                "https://www.bilibili.com".to_string()
            } else {
                format!("https://www.bilibili.com/video/{}", bvid)
            };

            let description_html = captura_rules::bilibili::utils::render_ugc_description(
                false,
                cover,
                &summary,
                Some(bvid),
                None,
            );

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: e.author.clone(),
                pub_date: None,
                categories: vec!["bilibili".to_string(), "ranking".to_string()],
            });
        }

        let title = if rid_numeric == "0" {
            "bilibili 排行榜-全站".to_string()
        } else {
            format!("bilibili 排行榜-rid {}", rid_numeric)
        };

        let data = HubData {
            title,
            link: Some("https://www.bilibili.com/v/popular/rank/all".to_string()),
            description: None,
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_ranking hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_RANKING_HANDLER: BilibiliRankingHubHandler = BilibiliRankingHubHandler;

/// Built-in Hub handler for bilibili/user/video.
pub struct BilibiliUserVideoHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliUserVideoHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let uid = ctx.param_str("uid").unwrap_or("");
        if uid.is_empty() {
            return Err(captura_common::Error::Config(
                "uid is required for bilibili/user/video".into(),
            ));
        }
        // Missing `embed` parameter means "embed enabled", matching other routes.
        let embed = ctx.param_str("embed").is_none();

        let mut spec = captura_rules::bilibili::bilibili_user_video_rule();
        if let Some(params) = &mut spec.params {
            params
                .defaults
                .insert("uid".to_string(), serde_json::json!(uid));
            params
                .defaults
                .insert("embed".to_string(), serde_json::json!(embed));
        }

        let feed_model = build_virtual_feed_for_spec(&spec);
        let entries = execute_json_v1(&feed_model, &spec).await?;

        let mut items = Vec::new();
        for e in entries {
            let title = e.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            let summary = e.summary.unwrap_or_default();
            let cover = e.content_html.as_deref();
            let bvid = e.url.as_deref();

            let link = if let Some(b) = bvid {
                format!("https://www.bilibili.com/video/{}", b)
            } else {
                format!("https://space.bilibili.com/{}", uid)
            };

            let description_html = captura_rules::bilibili::utils::render_ugc_description(
                embed, cover, &summary, bvid, None,
            );

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: e.author.clone(),
                pub_date: None,
                categories: vec!["bilibili".to_string(), "user-video".to_string()],
            });
        }

        let data = HubData {
            title: format!("{} 的 bilibili 空间", uid),
            link: Some(format!("https://space.bilibili.com/{}", uid)),
            description: Some(format!("{} 的 bilibili 空间", uid)),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_user_video hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_USER_VIDEO_HANDLER: BilibiliUserVideoHubHandler = BilibiliUserVideoHubHandler;

/// Built-in Hub handler for bilibili/user/dynamic (simplified dynamic feed).
pub struct BilibiliUserDynamicHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliUserDynamicHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let uid = ctx.param_str("uid").unwrap_or("");
        if uid.is_empty() {
            return Err(captura_common::Error::Config(
                "uid is required for bilibili/user/dynamic".into(),
            ));
        }
        let embed = ctx.param_str("embed").is_none();
        let direct_link = ctx
            .param_str("directLink")
            .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
            .unwrap_or(false);
        let use_avid = ctx
            .param_str("useAvid")
            .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
            .unwrap_or(false);
        let show_emoji = ctx
            .param_str("showEmoji")
            .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
            .unwrap_or(false);
        let hide_goods = ctx
            .param_str("hideGoods")
            .map(|v| matches!(v, "1" | "true" | "True" | "TRUE"))
            .unwrap_or(false);
        let offset = ctx.param_str("offset").map(|s| s.to_string());

        let opts = captura_rules::bilibili::dynamic::DynamicOptions {
            show_emoji,
            embed,
            hide_goods,
            direct_link,
            use_avid,
            offset,
        };

        let entries = captura_rules::bilibili::dynamic::fetch_user_dynamic(uid, &opts).await?;

        let mut items = Vec::new();
        for e in entries {
            let title = e.title.unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let description_html = e.content_html.clone().unwrap_or_default();
            let link = e
                .url
                .clone()
                .unwrap_or_else(|| format!("https://space.bilibili.com/{}/dynamic", uid));

            items.push(HubItem {
                title,
                description: Some(description_html),
                link: Some(link),
                author: e.author.clone(),
                pub_date: e
                    .published_at
                    .map(|d| d.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())),
                categories: Vec::new(),
            });
        }

        let data = HubData {
            title: format!("{} 的 bilibili 动态", uid),
            link: Some(format!("https://space.bilibili.com/{}/dynamic", uid)),
            description: Some(format!("{} 的 bilibili 动态", uid)),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_user_dynamic hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_USER_DYNAMIC_HANDLER: BilibiliUserDynamicHubHandler = BilibiliUserDynamicHubHandler;

/// Built-in Hub handler for bilibili/bangumi/season.
pub struct BilibiliBangumiSeasonHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliBangumiSeasonHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let season_id = ctx.param_str("season_id").unwrap_or("");
        if season_id.is_empty() {
            return Err(captura_common::Error::Config(
                "season_id is required for bilibili/bangumi/season".into(),
            ));
        }
        let embed = ctx.param_str("embed").is_none();

        let episodes = captura_rules::bilibili::utils::fetch_bangumi_episodes(season_id).await?;

        let mut items = Vec::new();
        for ep in episodes {
            if ep.full_title.is_empty() {
                continue;
            }
            let summary = ep.number.unwrap_or_default();
            let cover = ep.cover.as_deref();
            let url = ep.share_url.clone();

            let description_html = captura_rules::bilibili::utils::render_ugc_description(
                embed, cover, &summary, None, None,
            );

            items.push(HubItem {
                title: ep.full_title.clone(),
                description: Some(description_html),
                link: Some(url.clone()),
                author: None,
                pub_date: None,
                categories: vec!["bilibili".to_string(), "bangumi".to_string()],
            });
        }

        let data = HubData {
            title: format!("Bilibili Bangumi Season {}", season_id),
            link: Some(format!(
                "https://www.bilibili.com/bangumi?season_id={}",
                season_id
            )),
            description: None,
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_bangumi_season hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_BANGUMI_SEASON_HANDLER: BilibiliBangumiSeasonHubHandler =
    BilibiliBangumiSeasonHubHandler;

/// Built-in Hub handler for bilibili/bangumi/media.
pub struct BilibiliBangumiMediaHubHandler;

#[async_trait::async_trait]
impl HubHandler for BilibiliBangumiMediaHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let media_id = ctx.param_str("mediaid").unwrap_or("");
        if media_id.is_empty() {
            return Err(captura_common::Error::Config(
                "mediaid is required for bilibili/bangumi/media".into(),
            ));
        }
        let embed = ctx.param_str("embed").is_none();

        let meta = captura_rules::bilibili::utils::fetch_bangumi_media(media_id).await?;

        let episodes =
            captura_rules::bilibili::utils::fetch_bangumi_episodes(&meta.season_id).await?;

        let mut items = Vec::new();
        for ep in episodes {
            if ep.full_title.is_empty() {
                continue;
            }
            let summary = ep.number.clone().unwrap_or_default();
            let cover = ep.cover.as_deref();
            let url = ep.share_url.clone();

            let description_html = captura_rules::bilibili::utils::render_ugc_description(
                embed, cover, &summary, None, None,
            );

            items.push(HubItem {
                title: ep.full_title.clone(),
                description: Some(description_html),
                link: Some(url.clone()),
                author: None,
                pub_date: None,
                categories: vec!["bilibili".to_string(), "bangumi".to_string()],
            });
        }

        let title = meta.title;
        let description = meta.evaluate;
        let image = meta
            .cover
            .map(|c| captura_rules::bilibili::utils::normalize_cover_url(&c));
        let link = meta
            .share_url
            .unwrap_or_else(|| format!("https://www.bilibili.com/bangumi/media/md{}", media_id));

        let data = HubData {
            title,
            link: Some(link),
            description,
            image,
            language: Some("zh-cn".to_string()),
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "bilibili_bangumi_media hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static BILIBILI_BANGUMI_MEDIA_HANDLER: BilibiliBangumiMediaHubHandler =
    BilibiliBangumiMediaHubHandler;

fn build_virtual_feed_for_spec(spec: &RuleSpecV1) -> feed::Model {
    let now_utc = chrono::Utc::now();
    let offset = chrono::FixedOffset::east_opt(0).unwrap();
    let now = now_utc.with_timezone(&offset);
    let feed_url = spec
        .source
        .request
        .as_ref()
        .map(|r| r.url.clone())
        .unwrap_or_else(|| "about:blank".to_string());

    feed::Model {
        id: 0,
        user_id: 0,
        category_id: None,
        r#type: feed::FeedType::Rule,
        title: spec.description.clone(),
        site_url: None,
        feed_url,
        favicon_id: None,
        rule_id: None,
        rule_params_json: None,
        user_agent: None,
        username: None,
        password: None,
        headers_json: None,
        cookies: None,
        proxy_url: None,
        fetch_via_proxy: false,
        disable_http2: false,
        allow_invalid_certs: false,
        request_timeout_ms: spec.fetch.timeout_ms.map(|v| v as i32),
        checked_at: None,
        next_run_at: None,
        etag: None,
        last_modified: None,
        last_status: None,
        error_count: 0,
        last_error_message: None,
        disabled: false,
        scraper_rules: None,
        rewrite_rules: None,
        blocklist_rules: None,
        keeplist_rules: None,
        url_rewrite_rules: None,
        block_filter_entry_rules: None,
        keep_filter_entry_rules: None,
        integrations_json: None,
        created_at: now,
        updated_at: now,
    }
}

fn build_bilibili_search_link(keyword: &str) -> String {
    let mut qs = url::form_urlencoded::Serializer::new(String::new());
    qs.append_pair("keyword", keyword);
    qs.append_pair("from_source", "webtop_search");
    format!("https://search.bilibili.com/all?{}", qs.finish())
}

/// Built-in Hub handler for zhihu/hotlist.
pub struct ZhihuHotlistHubHandler;

#[async_trait::async_trait]
impl HubHandler for ZhihuHotlistHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://www.zhihu.com/hot".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let mut opts = hub_utils::HubHttpOpts::default();
        // Zhihu uses stronger anti-crawling; UA/headers can be extended here in the future if needed.
        opts.smart = false;
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "div.HotItem", |el| {
            let link = crate::extract_attr(&el, "a.HotItem-title@href")
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "a.HotItem-title");
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Zhihu Hot List".to_string(),
            description: Some("Zhihu hot list entries.".to_string()),
            link: Some("https://www.zhihu.com/hot".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "zhihu_hotlist hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static ZHIHU_HOTLIST_HANDLER: ZhihuHotlistHubHandler = ZhihuHotlistHubHandler;

/// Built-in Hub handler for reuters/top.
pub struct ReutersTopHubHandler;

#[async_trait::async_trait]
impl HubHandler for ReutersTopHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let url = "https://www.reuters.com/world/".to_string();

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "article.story-card, article.story", |el| {
            let link =
                crate::extract_attr(&el, "a@href").map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h3").or_else(|| crate::extract_text(&el, "h2"));
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: "Reuters Top News".to_string(),
            description: Some("Reuters top news stories.".to_string()),
            link: Some("https://www.reuters.com/world/".to_string()),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "reuters_top hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static REUTERS_TOP_HANDLER: ReutersTopHubHandler = ReutersTopHubHandler;

/// Built-in Hub handler for medium/tag.
pub struct MediumTagHubHandler;

#[async_trait::async_trait]
impl HubHandler for MediumTagHubHandler {
    async fn handle(&self, ctx: &mut HubHandlerCtx<'_>) -> captura_common::Result<HubResult> {
        let tag = ctx.param_str("tag").unwrap_or("rust");
        let url = format!("https://medium.com/tag/{}/latest", tag);

        let mut items: Vec<HubItem> = Vec::new();
        let opts = hub_utils::HubHttpOpts::default();
        let html = hub_utils::get_html(&url, &opts, None).await?;

        hub_utils::for_each_element(&html, "div.postArticle", |el| {
            let link = crate::extract_attr(&el, "a.ds-link@href")
                .or_else(|| crate::extract_attr(&el, "a.link--primary@href"))
                .map(|href| hub_utils::absolutize(&url, &href));
            let title = crate::extract_text(&el, "h3").or_else(|| crate::extract_text(&el, "h2"));
            let desc_html = hub_utils::element_html_sanitized(&el);
            items.push(HubItem {
                title: title.unwrap_or_else(|| link.clone().unwrap_or_default()),
                description: Some(desc_html),
                link,
                author: None,
                pub_date: None,
                categories: Vec::new(),
            });
        })?;

        let data = HubData {
            title: format!("Medium Tag: {}", tag),
            description: Some("Medium posts by tag.".to_string()),
            link: Some(format!("https://medium.com/tag/{}", tag)),
            image: None,
            language: None,
            items,
            allow_empty: false,
        };

        debug!(
            hub_id = ctx.hub_id,
            items = data.items.len(),
            "medium_tag hub handler"
        );

        Ok(HubResult::Data(data))
    }
}

static MEDIUM_TAG_HANDLER: MediumTagHubHandler = MediumTagHubHandler;

fn hub_result_to_entries(res: HubResult) -> Result<Vec<NormalizedEntry>> {
    match res {
        HubResult::Data(HubData { items, .. }) => {
            let mut entries = Vec::new();
            for item in items {
                let url = item.link.clone();
                entries.push(NormalizedEntry {
                    guid: url.clone(),
                    url,
                    title: Some(item.title),
                    summary: item.description.clone(),
                    content_html: item.description,
                    author: item.author,
                    published_at: item.pub_date.map(|d| d.with_timezone(&chrono::Utc)),
                    enclosures: Vec::new(),
                    extras: serde_json::json!({}),
                });
            }
            Ok(entries)
        }
    }
}

/// Try executing a built-in Hub handler for a given rule spec (mapped by rule id).
pub(crate) async fn execute_builtin_hub_for_rule(
    feed: &feed::Model,
    spec: &RuleSpecV1,
) -> Option<Result<Vec<NormalizedEntry>>> {
    let hub_id = if let Some(rest) = spec.id.strip_prefix("captura.route.") {
        rest.replace('.', "/")
    } else {
        return None;
    };

    let params = merge_rule_params_v1(spec, feed.rule_params_json.as_ref());
    let mut param_map = serde_json::Map::new();
    if let Some(val) = params {
        if let Some(obj) = val.as_object() {
            param_map = obj.clone();
        }
    }

    let mut ctx = HubHandlerCtx {
        hub_id: &hub_id,
        params: &param_map,
    };

    let handler = match find_builtin_handler(&hub_id) {
        Some(h) => h,
        None => return None,
    };

    let res = handler.handle(&mut ctx).await;
    Some(res.and_then(hub_result_to_entries))
}

/// Execute a Hub route by its id and parameters, returning `HubResult`.
pub async fn execute_hub_route(
    hub_id: &str,
    params: &serde_json::Map<String, serde_json::Value>,
) -> captura_common::Result<HubResult> {
    let handler = find_builtin_handler(hub_id)
        .ok_or_else(|| captura_common::Error::Config(format!("unknown hub route: {}", hub_id)))?;
    let mut ctx = HubHandlerCtx { hub_id, params };
    handler.handle(&mut ctx).await
}
