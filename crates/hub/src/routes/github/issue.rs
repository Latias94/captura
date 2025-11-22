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
struct GithubIssue {
    title: String,
    body: Option<String>,
    html_url: String,
    number: i64,
    created_at: String,
    user: GithubUser,
    #[serde(default)]
    pull_request: Option<serde_json::Value>,
}

pub const META_GITHUB_ISSUE: RouteMeta = RouteMeta {
    hub_id: "github/issue",
    path: "/github/issue/:user/:repo/:state?/:labels?",
    categories: &["programming"],
    example: "/github/issue?user=DIYgod&repo=RSSHub&state=open",
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
            description: "Issue state: open / closed / all (default open)",
            default: Some("open"),
            options: &[
                ("open", "Open"),
                ("closed", "Closed"),
                ("all", "All"),
            ],
        },
        ParamMeta {
            name: "labels",
            description: "Comma-separated label names used to filter issues",
            default: None,
            options: &[],
        },
        ParamMeta {
            name: "limit",
            description: "Maximum number of issues to fetch (1-100, default 50)",
            default: Some("50"),
            options: &[],
        },
    ],
    features: Features {
        require_config: &[
            FeatureConfig {
                name: "GITHUB_ACCESS_TOKEN",
                description:
                    "GitHub access token (optional) used to authenticate API requests and avoid rate limits.",
                optional: true,
            },
        ],
        ..Features::basic()
    },
    radar: &[Radar {
        source: &[
            "github.com/:user/:repo/issues",
            "github.com/:user/:repo/issues/:id",
            "github.com/:user/:repo",
        ],
        target: "/issue/:user/:repo",
    }],
    name: "Repo Issues",
    maintainers: &["captura"],
    url: "https://github.com",
    description: "GitHub repository issues via the REST API, aligned with RSSHub /github/issue.",
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
            "github/issue: parameters `user` and `repo` are required".to_string(),
        ));
    }

    let host = format!("https://github.com/{}/{}", user, repo);
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
        .map_err(|e| Error::Network(format!("github issues: {}", e)))?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!(
            "github issues: http status {}",
            resp.status()
        )));
    }
    let issues: Vec<GithubIssue> = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;

    let mut items = Vec::new();
    for issue in issues.into_iter().filter(|i| i.pull_request.is_none()) {
        let title = issue.title;
        let link = format!("{}/issues/{}", host, issue.number);
        let body_html = issue.body.map(|b| {
            let escaped = html_escape::encode_safe(b.trim());
            format!("<pre>{}</pre>", escaped)
        });
        let pub_date = parse_datetime(&issue.created_at);

        items.push(HubItem {
            title,
            description: body_html,
            link: Some(link),
            author: Some(issue.user.login),
            pub_date,
            categories: vec!["github".to_string(), "issue".to_string()],
        });
    }

    let mut title_suffix = String::new();
    if !state.is_empty() {
        title_suffix.push(' ');
        title_suffix.push_str(&state.to_lowercase());
        title_suffix.push_str(" issues");
    }
    if !labels.is_empty() {
        title_suffix.push_str(" (");
        title_suffix.push_str(labels);
        title_suffix.push(')');
    }

    Ok(HubData {
        title: format!("{}/{}{}", user, repo, title_suffix),
        description: Some(format!(
            "GitHub issues for {}/{}{}",
            user, repo, title_suffix
        )),
        link: Some(format!("{}/issues", host)),
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
pub const ROUTE_GITHUB_ISSUE: Route = Route {
    meta: &META_GITHUB_ISSUE,
    handler: handler_fn,
};
