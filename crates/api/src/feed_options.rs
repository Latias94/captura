use sea_orm::Set;

use captura_storage::entity::feed;

use crate::error::{bad_request, ApiResult};

/// 统一封装订阅源可更新的抓取/规则等选项，便于在不同 API 层复用逻辑。
pub(crate) struct FeedUpdateOptions {
    pub user_agent: Option<String>,
    pub headers_json: Option<serde_json::Value>,
    pub cookies: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: Option<bool>,
    pub disable_http2: Option<bool>,
    pub allow_invalid_certs: Option<bool>,
    pub request_timeout_ms: Option<i32>,
    pub integrations_json: Option<serde_json::Value>,
    pub rule_params_json: Option<serde_json::Value>,
    pub username: Option<String>,
    pub password: Option<String>,
    // 规则相关字段（主要供 Miniflux 兼容层使用）
    pub scraper_rules: Option<String>,
    pub rewrite_rules: Option<String>,
    pub blocklist_rules: Option<String>,
    pub keeplist_rules: Option<String>,
    pub url_rewrite_rules: Option<String>,
    // Miniflux 兼容字段：允许更新 feed_url/site_url
    pub feed_url: Option<String>,
    pub site_url: Option<String>,
}

/// 将 FeedUpdateOptions 应用到 ActiveModel 上，负责统一的校验与空值处理。
pub(crate) fn apply_feed_update_options(
    am: &mut feed::ActiveModel,
    opts: FeedUpdateOptions,
) -> ApiResult<()> {
    if let Some(ua) = opts.user_agent {
        if ua.trim().is_empty() {
            am.user_agent = Set(None);
        } else {
            am.user_agent = Set(Some(ua));
        }
    }
    if let Some(u) = opts.username {
        if u.trim().is_empty() {
            am.username = Set(None);
        } else {
            am.username = Set(Some(u));
        }
    }
    if let Some(p) = opts.password {
        if p.trim().is_empty() {
            am.password = Set(None);
        } else {
            am.password = Set(Some(p));
        }
    }
    if let Some(h) = opts.headers_json {
        am.headers_json = Set(Some(h));
    }
    if let Some(c) = opts.cookies {
        if c.trim().is_empty() {
            am.cookies = Set(None);
        } else {
            am.cookies = Set(Some(c));
        }
    }
    if let Some(p) = opts.proxy_url {
        if p.trim().is_empty() {
            am.proxy_url = Set(None);
        } else {
            am.proxy_url = Set(Some(p));
        }
    }
    if let Some(v) = opts.fetch_via_proxy {
        am.fetch_via_proxy = Set(v);
    }
    if let Some(v) = opts.disable_http2 {
        am.disable_http2 = Set(v);
    }
    if let Some(v) = opts.allow_invalid_certs {
        am.allow_invalid_certs = Set(v);
    }
    if let Some(v) = opts.request_timeout_ms {
        am.request_timeout_ms = Set(Some(v));
    }
    if let Some(v) = opts.integrations_json {
        if !v.is_object() {
            return Err(bad_request("integrations_json must be an object"));
        }
        am.integrations_json = Set(Some(v));
    }
    if let Some(v) = opts.rule_params_json {
        if !v.is_object() {
            return Err(bad_request("rule_params_json must be an object"));
        }
        am.rule_params_json = Set(Some(v));
    }
    if let Some(s) = opts.scraper_rules {
        let s = s.trim();
        am.scraper_rules = if s.is_empty() {
            Set(None)
        } else {
            Set(Some(s.to_string()))
        };
    }
    if let Some(s) = opts.rewrite_rules {
        let s = s.trim();
        am.rewrite_rules = if s.is_empty() {
            Set(None)
        } else {
            Set(Some(s.to_string()))
        };
    }
    if let Some(s) = opts.blocklist_rules {
        let s = s.trim();
        am.blocklist_rules = if s.is_empty() {
            Set(None)
        } else {
            Set(Some(s.to_string()))
        };
    }
    if let Some(s) = opts.keeplist_rules {
        let s = s.trim();
        am.keeplist_rules = if s.is_empty() {
            Set(None)
        } else {
            Set(Some(s.to_string()))
        };
    }
    if let Some(s) = opts.url_rewrite_rules {
        let s = s.trim();
        am.url_rewrite_rules = if s.is_empty() {
            Set(None)
        } else {
            Set(Some(s.to_string()))
        };
    }
    if let Some(s) = opts.feed_url {
        if !s.trim().is_empty() {
            am.feed_url = Set(s);
        }
    }
    if let Some(s) = opts.site_url {
        let s = s.trim();
        am.site_url = if s.is_empty() {
            Set(None)
        } else {
            Set(Some(s.to_string()))
        };
    }
    Ok(())
}
