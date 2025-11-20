use crate::routes::types::{
    FeatureConfig, Features, HubCtx, HubData, HubItem, Radar, Route, RouteMeta,
};
use crate::routes::util;
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use serde::Deserialize;
use std::collections::HashMap;

pub const META_GITHUB_TRENDING: RouteMeta = RouteMeta {
    hub_id: "github/trending",
    path: "/github/trending/:since/:language/:spoken_language?",
    categories: &["programming"],
    example: "/github/trending/daily/javascript/en",
    params: &[
        crate::routes::types::ParamMeta {
            name: "since",
            description: "time range: daily / weekly / monthly",
            default: Some("daily"),
            options: &[
                ("daily", "Today"),
                ("weekly", "This week"),
                ("monthly", "This month"),
            ],
        },
        crate::routes::types::ParamMeta {
            name: "language",
            description:
                "repository language slug in /trending/{language}; use 'any' or empty for all languages",
            default: Some("any"),
            options: &[],
        },
        crate::routes::types::ParamMeta {
            name: "spoken_language",
            description:
                "spoken_language_code in trending URL; empty for all spoken languages",
            default: None,
            options: &[],
        },
    ],
    features: Features::with_config(&[
        FeatureConfig {
            name: "GITHUB_ACCESS_TOKEN",
            description: "GitHub access token used by the route (optional in Captura, required in some environments)",
            optional: true,
        },
    ]),
    radar: &[
        Radar {
            source: &["github.com/trending"],
            target: "/trending/:since",
        },
    ],
    name: "Trending",
    maintainers: &["captura"],
    url: "https://github.com/trending",
    description: "GitHub Trending repositories (inspired by RSSHub github/trending route).",
    default_view: Some("social"),
};

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
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

    let html = util::get_html(&url).await?;

    let mut repos = Vec::new();
    util::for_each_element(&html, "article.Box-row", |el| {
        let href = util::extract_attr(&el, "h2 a@href");
        let link = href
            .as_deref()
            .map(|h| util::absolutize("https://github.com", h));
        let title = util::extract_text(&el, "h2 a");
        let owner_repo = link
            .as_deref()
            .and_then(|l| parse_owner_repo(l, title.as_deref()));
        let (owner, name) = match owner_repo {
            Some((o, n)) => (Some(o), Some(n)),
            None => (None, None),
        };

        repos.push(TrendingRepo {
            owner,
            name,
            link,
            title,
            article_html: util::element_html(&el),
        });
    })?;

    let mut details_map = HashMap::new();
    if let Ok(token) = std::env::var("GITHUB_ACCESS_TOKEN") {
        if !token.is_empty() {
            if let Ok(map) = fetch_github_repo_details(&repos, &token).await {
                details_map = map;
            } else {
                // When GraphQL enrichment fails, we fall back to HTML-only items.
            }
        }
    }

    let items = build_items_from_trending(repos, details_map);

    Ok(HubData {
        title: extract_page_title(&html).unwrap_or_else(|| "GitHub Trending".to_string()),
        description: Some("GitHub trending repositories".to_string()),
        link: Some(url),
        image: None,
        language: None,
        items,
        allow_empty: false,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GITHUB_TRENDING: Route = Route {
    meta: &META_GITHUB_TRENDING,
    handler: handler_fn,
};

#[derive(Debug)]
struct TrendingRepo {
    owner: Option<String>,
    name: Option<String>,
    link: Option<String>,
    title: Option<String>,
    article_html: String,
}

#[derive(Debug, Deserialize)]
struct GraphqlResponse {
    data: HashMap<String, RepoDetails>,
}

#[derive(Debug, Deserialize)]
struct RepoDetails {
    description: Option<String>,
    forkCount: Option<i64>,
    nameWithOwner: String,
    openGraphImageUrl: Option<String>,
    primaryLanguage: Option<RepoLanguage>,
    stargazerCount: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RepoLanguage {
    name: String,
}

fn parse_owner_repo(link: &str, title: Option<&str>) -> Option<(String, String)> {
    if let Ok(url) = url::Url::parse(link) {
        let mut segments = url.path().trim_matches('/').split('/');
        if let (Some(owner), Some(name)) = (segments.next(), segments.next()) {
            return Some((owner.to_string(), name.to_string()));
        }
    }
    if let Some(t) = title {
        let parts: Vec<&str> = t.split('/').map(|s| s.trim()).collect();
        if parts.len() >= 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    None
}

async fn fetch_github_repo_details(
    repos: &[TrendingRepo],
    token: &str,
) -> Result<HashMap<String, RepoDetails>, Error> {
    use std::collections::HashSet;
    use std::fmt::Write as _;

    let mut repo_identifiers: Vec<(String, String)> = Vec::new();
    let mut seen = HashSet::new();

    for repo in repos {
        if let (Some(owner), Some(name), Some(_link)) = (&repo.owner, &repo.name, &repo.link) {
            let key = format!("{}/{}", owner, name);
            if seen.insert(key) {
                repo_identifiers.push((owner.clone(), name.clone()));
            }
        }
    }

    if repo_identifiers.is_empty() {
        return Ok(HashMap::new());
    }

    let mut query_repos = String::new();
    for (idx, (owner, name)) in repo_identifiers.iter().enumerate() {
        let _ = writeln!(
            &mut query_repos,
            "  _{idx}: repository(owner: \"{owner}\", name: \"{name}\") {{ ...RepositoryFragment }}",
        );
    }

    let query = format!(
        "query {{\n{repos}}}\n\nfragment RepositoryFragment on Repository {{\n  description\n  forkCount\n  nameWithOwner\n  openGraphImageUrl\n  primaryLanguage {{ name }}\n  stargazerCount\n}}\n",
        repos = query_repos,
    );

    let body = serde_json::json!({ "query": query });

    let client = captura_net::client_basic(None, None)
        .map_err(|e| Error::Network(format!("github graphql client: {}", e)))?;
    let resp = client
        .post("https://api.github.com/graphql")
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("https://api.github.com/graphql -> {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Network(format!(
            "https://api.github.com/graphql -> http status {}",
            status
        )));
    }

    let gql: GraphqlResponse = resp
        .json()
        .await
        .map_err(|e| Error::Parse(format!("github graphql: {}", e)))?;

    // Re-key by "owner/name" to match how we look them up later.
    let mut out = HashMap::new();
    for (_alias, details) in gql.data {
        out.insert(details.nameWithOwner.clone(), details);
    }
    Ok(out)
}

fn build_items_from_trending(
    repos: Vec<TrendingRepo>,
    mut details_map: HashMap<String, RepoDetails>,
) -> Vec<HubItem> {
    let mut items = Vec::new();

    for repo in repos {
        let link = repo.link.clone();
        let author = repo.owner.clone();
        let key = match (&repo.owner, &repo.name) {
            (Some(o), Some(n)) => Some(format!("{}/{}", o, n)),
            _ => None,
        };

        if let Some(ref k) = key {
            if let Some(details) = details_map.remove(k) {
                let lang = details
                    .primaryLanguage
                    .as_ref()
                    .map(|l| l.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                let stars = details.stargazerCount.unwrap_or(0);
                let forks = details.forkCount.unwrap_or(0);
                let cover = details.openGraphImageUrl;
                let desc = details.description.unwrap_or_default();

                let mut desc_html = String::new();
                if let Some(cover_url) = cover {
                    desc_html.push_str(&format!(r#"<img src="{cover}"/>"#, cover = cover_url));
                    desc_html.push_str("<br>");
                }
                desc_html.push_str(&desc);
                desc_html.push_str("<br><br>");
                desc_html.push_str(&format!(
                    "Language: {lang}<br>Stars: {stars}<br>Forks: {forks}"
                ));

                items.push(HubItem {
                    title: details.nameWithOwner,
                    description: Some(desc_html),
                    link,
                    author: author.clone(),
                    pub_date: None,
                    categories: vec![lang],
                });

                continue;
            }
        }

        // Fallback to HTML-only item when GraphQL data is missing.
        let fallback_title = if let Some(ref k) = key {
            k.clone()
        } else if let Some(t) = &repo.title {
            normalize_whitespace(t)
        } else if let Some(l) = &link {
            l.clone()
        } else {
            "GitHub repository".to_string()
        };

        items.push(HubItem {
            title: fallback_title,
            description: Some(repo.article_html.clone()),
            link,
            author,
            pub_date: None,
            categories: Vec::new(),
        });
    }

    items
}

fn extract_page_title(html: &str) -> Option<String> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let sel = Selector::parse("title").ok()?;
    let el = doc.select(&sel).next()?;
    let text = el.text().collect::<String>().trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn normalize_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_is_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !last_is_space {
                out.push(' ');
                last_is_space = true;
            }
        } else {
            last_is_space = false;
            out.push(ch);
        }
    }
    out.trim().to_string()
}
