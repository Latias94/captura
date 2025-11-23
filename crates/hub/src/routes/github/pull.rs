use crate::routes::types::{
    FeatureConfig, Features, HubCtx, HubData, HubItem, ParamMeta, Radar, Route, RouteMeta,
};
use captura_common::Error;
use captura_hub_macros::register_hub_route;
use captura_net::client_basic;
use chrono::{DateTime, FixedOffset};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GithubUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GithubIssueOrPr {
    title: String,
    body: Option<String>,
    html_url: String,
    created_at: String,
    user: GithubUser,
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

pub const META_GITHUB_PULL: RouteMeta = RouteMeta {
    hub_id: "github/pull",
    path: "/github/pull/:user/:repo/:state?/:labels?",
    categories: &["programming"],
    example: "/github/pull?user=DIYgod&repo=RSSHub&state=open",
    params: &[
        ParamMeta {
            name: "user",
            description: "GitHub username or organization",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "repo",
            description: "GitHub repository name",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "state",
            description: "Pull request state: open / closed / all (default open)",
            default: Some("open"),
            options: &[("open", "Open"), ("closed", "Closed"), ("all", "All")],
        },
        ParamMeta {
            name: "labels",
            description: "Comma-separated label names used to filter pull requests",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "Maximum number of pull requests to fetch (1-100, default 50)",
            default: Some("50"),
            options: &[],
        },
    ],
    features: Features {
        require_config: &[FeatureConfig {
            name: "GITHUB_ACCESS_TOKEN",
            description: "GitHub access token (optional) used to authenticate API requests and avoid rate limits.",
            optional: true,
        }],
        ..Features::basic()
    },
    radar: &[Radar {
        source: &[
            "github.com/:user/:repo/pulls",
            "github.com/:user/:repo/pulls/:id",
            "github.com/:user/:repo",
        ],
        target: "/pull/:user/:repo",
    }],
    name: "Repo Pull Requests",
    maintainers: &["captura"],
    url: "https://github.com",
    description: "GitHub repository pull requests via the REST API, aligned with RSSHub /github/pull.",
    default_view: Some("program-update"),
};

fn parse_datetime(s: &str) -> Option<DateTime<FixedOffset>> {
    crate::routes::util::parse_date(s)
}

pub async fn handler(ctx: &mut HubCtx<'_>) -> captura_common::Result<HubData> {
    let user = ctx.param_str("user").unwrap_or("").trim();
    let repo = ctx.param_str("repo").unwrap_or("").trim();
    let state = ctx.param_str("state").unwrap_or("open").trim();
    let labels = ctx.param_str("labels").unwrap_or("").trim();
    let limit = ctx.param_i64("limit").unwrap_or(50).clamp(1, 100) as usize;

    if user.is_empty() || repo.is_empty() {
        return Err(Error::Config(
            "github/pull: parameters `user` and `repo` are required".to_string(),
        ));
    }

    let host = format!("https://github.com/{}/{}", user, repo);
    let pulls_link = format!("{}/pulls", host);
    let api_url = format!("https://api.github.com/repos/{}/{}/issues", user, repo);

    let client = client_basic(None, None)
        .map_err(|e| Error::Network(format!("github client error: {}", e)))?;

    let mut req = client
        .get(&api_url)
        .header("Accept", "application/vnd.github.v3+json")
        .query(&[
            ("state", state),
            ("sort", "created"),
            ("direction", "desc"),
            ("per_page", &limit.to_string()),
        ]);

    if !labels.is_empty() {
        req = req.query(&[("labels", labels)]);
    }

    if let Ok(token) = std::env::var("GITHUB_ACCESS_TOKEN") {
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| Error::Network(format!("github pulls: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "github pulls: http status {}",
            resp.status()
        )));
    }
    let issues: Vec<GithubIssueOrPr> =
        resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for pr in issues.into_iter().filter(|i| i.pull_request.is_some()) {
        let title = pr.title;
        let link = pr.html_url.clone();
        let body_html = pr.body.map(|b| {
            let escaped = html_escape::encode_safe(b.trim());
            format!("<pre>{}</pre>", escaped)
        });
        let pub_date = parse_datetime(&pr.created_at);

        items.push(HubItem {
            title,
            description: body_html,
            link: Some(link),
            author: Some(pr.user.login),
            pub_date,
            categories: vec!["github".to_string(), "pull".to_string()],
        });
    }

    let mut title_suffix = String::from(" pull requests");
    if !state.is_empty() {
        title_suffix.push_str(&format!(" ({})", state.to_lowercase()));
    }
    if !labels.is_empty() {
        title_suffix.push_str(" [");
        title_suffix.push_str(labels);
        title_suffix.push(']');
    }

    Ok(HubData {
        title: format!("{}/{}{}", user, repo, title_suffix),
        description: Some(format!(
            "GitHub pull requests for {}/{}{}",
            user, repo, title_suffix
        )),
        link: Some(pulls_link),
        image: None,
        language: Some("en".to_string()),
        items,
        allow_empty: true,
    })
}

fn handler_fn<'a>(ctx: &'a mut HubCtx<'a>) -> crate::routes::types::HubHandlerFuture<'a> {
    Box::pin(handler(ctx))
}

#[register_hub_route]
pub const ROUTE_GITHUB_PULL: Route = Route {
    meta: &META_GITHUB_PULL,
    handler: handler_fn,
};
