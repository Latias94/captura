//! Feed/JSON fetcher and parser.
//! This crate handles standard RSS/Atom/JSON feeds.

use captura_common::{Error, Result};
use feed_rs::model::Feed as ParsedFeed;
use feed_rs::parser;
use reqwest::header::{HeaderMap, ACCEPT, IF_MODIFIED_SINCE, IF_NONE_MATCH, USER_AGENT};
use reqwest::{Client, StatusCode};
use std::time::Duration;
use tracing::instrument;

#[derive(Clone, Debug, Default)]
pub struct FetchOptions {
    pub user_agent: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub headers: HeaderMap,
    pub timeout: Option<Duration>,
    pub allow_invalid_certs: bool,
    pub disable_http2: bool,
    pub proxy_url: Option<String>,
}

pub trait FeedFetcher: Send + Sync {
    fn client(&self) -> &Client;
    fn options(&self) -> &FetchOptions;
}

#[derive(Clone)]
pub struct HttpFetcher {
    client: Client,
    options: FetchOptions,
}

impl HttpFetcher {
    pub fn new(options: FetchOptions) -> Result<Self> {
        let mut builder = Client::builder();
        if let Some(t) = options.timeout {
            builder = builder.timeout(t);
        }
        if options.allow_invalid_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }
        if options.disable_http2 {
            builder = builder.http1_only();
        }
        if let Some(ref p) = options.proxy_url {
            if !p.is_empty() {
                if let Ok(proxy) = reqwest::Proxy::all(p) {
                    builder = builder.proxy(proxy);
                }
            }
        }
        let client = builder.build().map_err(|e| Error::Network(e.to_string()))?;
        Ok(Self { client, options })
    }

    #[instrument(skip(self))]
    pub async fn fetch_bytes_with_meta(
        &self,
        url: &str,
    ) -> Result<(Vec<u8>, HeaderMap, StatusCode)> {
        let mut req = self.client.get(url);
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            "application/rss+xml, application/atom+xml, application/json, text/xml, */*"
                .parse()
                .unwrap(),
        );
        if let Some(ua) = &self.options.user_agent {
            headers.insert(USER_AGENT, ua.parse().unwrap());
        }
        if let Some(etag) = &self.options.etag {
            headers.insert(IF_NONE_MATCH, etag.parse().unwrap());
        }
        if let Some(lm) = &self.options.last_modified {
            headers.insert(IF_MODIFIED_SINCE, lm.parse().unwrap());
        }
        headers.extend(self.options.headers.clone());
        req = req.headers(headers.clone());
        let resp = req
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        let status = resp.status();
        let hdrs = resp.headers().clone();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        Ok((bytes.to_vec(), hdrs, status))
    }

    #[instrument(skip(self))]
    pub async fn fetch_bytes(&self, url: &str) -> Result<(Vec<u8>, HeaderMap)> {
        let (b, h, s) = self.fetch_bytes_with_meta(url).await?;
        if s == StatusCode::NOT_MODIFIED {
            return Err(Error::Network("not modified".into()));
        }
        Ok((b, h))
    }

    #[instrument(skip(self))]
    pub async fn fetch_feed(&self, url: &str) -> Result<ParsedFeed> {
        let (bytes, _hdrs, status) = self.fetch_bytes_with_meta(url).await?;
        if status == StatusCode::NOT_MODIFIED {
            return Err(Error::Network("not modified".into()));
        }
        parser::parse(bytes.as_slice()).map_err(|e| Error::Parse(e.to_string()))
    }

    #[instrument(skip(self))]
    pub async fn fetch_feed_with_meta(&self, url: &str) -> Result<FeedFetchResult> {
        let (bytes, hdrs, status) = self.fetch_bytes_with_meta(url).await?;
        let etag = hdrs
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let last_modified = hdrs
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        if status == StatusCode::NOT_MODIFIED {
            return Ok(FeedFetchResult {
                meta: FeedFetchMeta {
                    status,
                    etag,
                    last_modified,
                },
                feed: None,
            });
        }
        if !status.is_success() {
            return Err(Error::Network(format!("http status {}", status)));
        }
        let parsed = parser::parse(bytes.as_slice()).map_err(|e| Error::Parse(e.to_string()))?;
        Ok(FeedFetchResult {
            meta: FeedFetchMeta {
                status,
                etag,
                last_modified,
            },
            feed: Some(parsed),
        })
    }
}

#[derive(Debug, Clone)]
pub struct FeedFetchMeta {
    pub status: StatusCode,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FeedFetchResult {
    pub meta: FeedFetchMeta,
    pub feed: Option<ParsedFeed>,
}

impl FeedFetcher for HttpFetcher {
    fn client(&self) -> &Client {
        &self.client
    }
    fn options(&self) -> &FetchOptions {
        &self.options
    }
}
