use crate::routes::types::{
    Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Result;
use captura_fetcher::{FetchOptions, HttpFetcher};
use captura_hub_macros::register_hub_route;
use chrono::{DateTime, FixedOffset, Utc};
use scraper::{Html, Selector};

fn make_fetcher() -> Result<HttpFetcher> {
    HttpFetcher::new(FetchOptions::default())
}

fn to_fixed_offset(dt: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    FixedOffset::east_opt(0).map(|offset| dt.with_timezone(&offset))
}

pub const META_NPR_FULL: RouteMeta = RouteMeta {
    hub_id: "npr/full",
    path: "/npr/full/:endpoint?",
    categories: &["traditional-media"],
    example: "/npr/full/1001",
    params: &[
        ParamMeta {
            name: "endpoint",
            description: "NPR RSS 频道 ID，对应官方 RSS URL 中的数字部分，默认 1001（News）。",
            default: Some("1001"),
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "最大条目数量（默认 20）。",
            default: Some("20"),
            options: &[],
        },
    ],
    features: Features::basic(),
    radar: &[Radar {
        source: &[
            "www.npr.org/sections/news/",
            "feeds.npr.org/:endpoint/rss.xml",
        ],
        target: "/full/:endpoint?",
    }],
    name: "NPR News (Full Text + Audio)",
    maintainers: &["captura"],
    url: "https://www.npr.org/sections/news/",
    description: "NPR 新闻 RSS 的全文 + 音频扩展，对标 RSSHub /npr/full/:endpoint，自动将页面中的音频模块转换为可播放的 <audio>。",
    default_view: Some("articles"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let endpoint = ctx.param_str("endpoint").unwrap_or("1001");
    let limit = ctx.param_i64("limit").unwrap_or(20).max(1) as usize;
    let feed_url = format!("https://feeds.npr.org/{}/rss.xml", endpoint);

    let fetcher = make_fetcher()?;
    let feed = fetcher.fetch_feed(&feed_url).await?;

    let feed_title = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .unwrap_or_else(|| format!("NPR Topics: {}", endpoint));
    let feed_link = feed
        .links
        .get(0)
        .map(|l| l.href.clone())
        .unwrap_or_else(|| "https://www.npr.org/sections/news/".to_string());
    let feed_image = feed
        .icon
        .as_ref()
        .map(|i| i.uri.clone())
        .or_else(|| feed.logo.as_ref().map(|i| i.uri.clone()))
        .or_else(|| {
            Some(
                "https://media.npr.org/images/podcasts/primary/npr_generic_image_300.jpg?s=200"
                    .to_string(),
            )
        });

    let mut items = Vec::new();

    'entries: for entry in feed.entries.into_iter().take(limit) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| entry.id.clone());
        let link = entry.links.get(0).map(|l| l.href.clone());

        // 初始描述使用 RSS 内容/摘要
        let mut description = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()));

        let mut author = if entry.authors.is_empty() {
            None
        } else {
            Some(
                entry
                    .authors
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };

        let mut categories = entry
            .categories
            .iter()
            .map(|c| c.term.clone())
            .collect::<Vec<_>>();

        // 抓取详情页，补充全文与音频
        if let Some(link_url) = &link {
            if let Ok(html) = util::get_html(link_url).await {
                let doc = Html::parse_document(&html);

                // 若页面显示 "audio-availability-message"，说明音频暂不可用，按照 RSSHub 做法跳过该条。
                if let Ok(sel_unavail) = Selector::parse(".audio-availability-message") {
                    if doc.select(&sel_unavail).next().is_some() {
                        continue 'entries;
                    }
                }

                // 替换/补充分类：优先使用页面上的 tag / slug
                if let Ok(sel_tag) = Selector::parse(".tag") {
                    let tags: Vec<String> = doc
                        .select(&sel_tag)
                        .filter_map(|el| {
                            let text = el.text().collect::<String>().trim().to_string();
                            if text.is_empty() { None } else { Some(text) }
                        })
                        .collect();
                    if !tags.is_empty() {
                        categories = tags;
                    } else if let Ok(sel_slug) = Selector::parse(".slug a") {
                        if let Some(el) = doc.select(&sel_slug).next() {
                            let text = el.text().collect::<String>().trim().to_string();
                            if !text.is_empty() {
                                categories = vec![text];
                            }
                        }
                    }
                }

                // 提取音频模块：将下载链接转换为 <audio> 播放器
                let mut audio_html = String::new();
                if let Ok(sel_module) = Selector::parse(".audio-module") {
                    for module in doc.select(&sel_module) {
                        if let Some(download_href) =
                            util::extract_attr(&module, ".audio-tool-download a@href")
                        {
                            if !download_href.trim().is_empty() {
                                audio_html.push_str(&format!(
                                    "<p>{}</p>",
                                    util::html_audio(download_href.trim())
                                ));
                            }
                        }
                    }
                }

                // headline 区域的主音频（如果存在）
                if let Ok(sel_head) = Selector::parse("#headlineaudio") {
                    if let Some(head) = doc.select(&sel_head).next() {
                        let head_html = util::element_html(&head);
                        if !head_html.trim().is_empty() {
                            audio_html =
                                format!("{head}{rest}", head = head_html, rest = audio_html);
                        }
                    }
                }

                // 整篇文章正文（storytext）
                let mut story_html = String::new();
                if let Ok(sel_story) = Selector::parse(".storytext") {
                    for el in doc.select(&sel_story) {
                        story_html.push_str(&util::element_html(&el));
                    }
                }

                if !audio_html.is_empty() || !story_html.is_empty() {
                    let mut full = String::new();
                    if !audio_html.is_empty() {
                        full.push_str(&audio_html);
                    }
                    if !story_html.is_empty() {
                        full.push_str(&story_html);
                    }
                    description = Some(full);
                }

                // 若页面上有更明确的作者信息（byline）可以覆盖 RSS 作者
                if let Ok(sel_byl) = Selector::parse("meta[name='byl']") {
                    if let Some(el) = doc.select(&sel_byl).next() {
                        if let Some(byl) = el.value().attr("content") {
                            let byl = byl.trim();
                            if !byl.is_empty() {
                                author = Some(byl.to_string());
                            }
                        }
                    }
                }
            }
        }

        let pub_date = entry.published.or(entry.updated).and_then(to_fixed_offset);

        items.push(HubItem {
            title,
            description,
            link,
            author,
            pub_date,
            categories,
        });
    }

    Ok(HubData {
        title: feed_title,
        description: Some(format!(
            "NPR full articles with embedded audio for feed {}.",
            endpoint
        )),
        link: Some(feed_link),
        image: feed_image,
        language: feed.language.clone(),
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_NPR_FULL: Route = Route {
    meta: &META_NPR_FULL,
    handler: handler_fn,
};
