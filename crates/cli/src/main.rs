use anyhow::{Context, Result, anyhow};
use captura_common::NormalizedEntry;
use captura_hub::{RuleSpecV1, parse_rule_v1};
use captura_storage::entity::feed;
use chrono::{FixedOffset, Utc};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "captura-cli", version, about = "Captura development CLI")]
struct Cli {
    /// Log level (RUST_LOG style), e.g. info, debug
    #[arg(long, global = true)]
    log: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Try running a DSL rule (rules v1) against a URL.
    ///
    /// This mirrors /api/v1/rules/try and is intended for local smoke testing
    /// whether a rule can successfully extract entries.
    RuleTry {
        /// Path to rule YAML file; use "-" to read from stdin.
        #[arg(long)]
        yaml: String,

        /// List URL to fetch (will override source.list.request.url in the rule).
        #[arg(long)]
        url: String,

        /// Optional HTTP proxy URL, e.g. http://127.0.0.1:10809
        #[arg(long)]
        proxy: Option<String>,

        /// Maximum number of entries to print.
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    /// Try running a built-in Hub route (RSSHub-style) by hub id.
    ///
    /// This calls the internal hub handler and prints a short summary of items.
    HubTry {
        /// Hub id, e.g. "hn/front", "v2ex/topics".
        #[arg(long)]
        hub: String,

        /// Optional JSON object of parameters, e.g. '{"section":"newest","view":"sources"}'.
        #[arg(long)]
        params: Option<String>,

        /// Maximum number of items to print.
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging: default to info, allow overrides via --log or RUST_LOG.
    let filter = if let Some(l) = &cli.log {
        EnvFilter::new(l.as_str())
    } else if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::from_default_env()
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt().with_env_filter(filter).init();

    match cli.command {
        Command::RuleTry {
            yaml,
            url,
            proxy,
            limit,
        } => {
            rule_try(&yaml, &url, proxy.as_deref(), limit).await?;
        }
        Command::HubTry { hub, params, limit } => {
            hub_try(&hub, params.as_deref(), limit).await?;
        }
    }

    Ok(())
}

async fn rule_try(yaml_path: &str, url: &str, proxy: Option<&str>, limit: usize) -> Result<()> {
    // 1) Read and parse YAML rule (using captura-hub's parse_rule_v1 + validate).
    let yaml = if yaml_path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("failed to read YAML from stdin")?;
        buf
    } else {
        std::fs::read_to_string(yaml_path)
            .with_context(|| format!("failed to read YAML file: {}", yaml_path))?
    };

    let mut spec: RuleSpecV1 = parse_rule_v1(&yaml)
        .with_context(|| format!("failed to parse rule YAML from {}", yaml_path))?;

    if !matches!(spec.source.kind, captura_hub::v1::SourceType::ListDetail) {
        return Err(anyhow!(
            "rule_try currently only supports source.type = list_detail"
        ));
    }

    // Override list.request.url with CLI-provided URL, same as /api/v1/rules/try.
    {
        let list = spec
            .source
            .list
            .as_mut()
            .ok_or_else(|| anyhow!("rule has no source.list section"))?;
        list.request.url = url.to_string();
    }

    // 2) Build a temporary feed::Model, reusing the logic from /api/v1/rules/try.
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let mut feed_model = feed::Model {
        id: 0,
        user_id: 0,
        category_id: None,
        r#type: feed::FeedType::Rule,
        title: Some("preview".into()),
        site_url: None,
        feed_url: url.to_string(),
        favicon_id: None,
        rule_id: None,
        rule_params_json: None,
        user_agent: spec.fetch.user_agent.clone(),
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
        // Feed-level rewrite/filter rules are ignored in the CLI scenario.
        view: spec.default_view.clone(),
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
    };

    // If a proxy is provided, apply it at feed level so that HTTP paths for
    // list_detail/single_page rules go through the proxy as well; JSON rules
    // use the effective_proxy logic inside rules_engine.
    if let Some(p) = proxy {
        feed_model.fetch_via_proxy = true;
        feed_model.proxy_url = Some(p.to_string());
    }

    // 3) Invoke pipeline::refresh_rule_v1 to execute the rule.
    let started = std::time::Instant::now();
    let entries: Vec<NormalizedEntry> =
        captura_pipeline::refresh_rule_v1(&feed_model, &spec).await?;
    let duration_ms = started.elapsed().as_millis();

    // 4) Print a human-readable summary so it is easy to see if the rule works.
    println!("Rule ID       : {}", spec.id);
    println!("List URL      : {}", url);
    println!(
        "Proxy         : {}",
        proxy.unwrap_or("<none> (set --proxy http://127.0.0.1:10809 if needed)")
    );
    println!(
        "Items         : {} (showing up to {})",
        entries.len(),
        limit
    );
    println!("Duration      : {} ms", duration_ms);
    println!();

    for (idx, e) in entries.iter().take(limit).enumerate() {
        let title = e.title.as_deref().unwrap_or("<no title>");
        let url = e.url.as_deref().unwrap_or("<no url>");
        let content_len = e.content_html.as_ref().map(|s| s.len()).unwrap_or(0);
        println!(
            "[{idx}] {title}\n     {url}\n     content_len={content_len}\n",
            idx = idx,
            title = title,
            url = url,
            content_len = content_len
        );
    }

    Ok(())
}

async fn hub_try(hub_id: &str, params_json: Option<&str>, limit: usize) -> Result<()> {
    let mut params_map = serde_json::Map::new();
    if let Some(s) = params_json {
        let v: serde_json::Value = serde_json::from_str(s)
            .with_context(|| format!("failed to parse params JSON: {}", s))?;
        if let Some(obj) = v.as_object() {
            params_map = obj.clone();
        } else {
            return Err(anyhow!("params must be a JSON object"));
        }
    }

    let data = captura_pipeline::execute_hub_route(hub_id, &params_map).await?;

    println!("Hub ID        : {}", hub_id);
    println!("Title         : {}", data.title);
    println!(
        "Link          : {}",
        data.link.clone().unwrap_or_else(|| "<none>".into())
    );
    println!(
        "Items         : {} (showing up to {})",
        data.items.len(),
        limit
    );
    println!();

    for (idx, item) in data.items.iter().take(limit).enumerate() {
        let title = &item.title;
        let link = item.link.as_deref().unwrap_or("<no link>");
        let desc_len = item.description.as_ref().map(|s| s.len()).unwrap_or(0);
        println!(
            "[{idx}] {title}\n     {link}\n     description_len={desc_len}\n",
            idx = idx,
            title = title,
            link = link,
            desc_len = desc_len
        );
    }

    Ok(())
}
