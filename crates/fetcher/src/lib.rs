//! Feed/JSON fetcher and parser.
//! This crate handles standard RSS/Atom/JSON feeds.

use captura_common::{Error, Result};
use feed_rs::model::Feed as ParsedFeed;
use feed_rs::parser;
use reqwest::header::{HeaderMap, ACCEPT, USER_AGENT};
use reqwest::Client;
use std::time::Duration;
use tracing::instrument;

#[derive(Clone, Debug, Default)]
pub struct FetchOptions {
    pub user_agent: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub headers: HeaderMap,
    pub timeout: Option<Duration>,
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
        let client = builder.build().map_err(|e| Error::Network(e.to_string()))?;
        Ok(Self { client, options })
    }

    #[instrument(skip(self))]
    pub async fn fetch_bytes(&self, url: &str) -> Result<(Vec<u8>, HeaderMap)> {
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
            headers.insert("If-None-Match", etag.parse().unwrap());
        }
        if let Some(lm) = &self.options.last_modified {
            headers.insert("If-Modified-Since", lm.parse().unwrap());
        }
        headers.extend(self.options.headers.clone());
        req = req.headers(headers.clone());
        let resp = req
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Err(Error::Network("not modified".into()));
        }
        let hdrs = resp.headers().clone();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Error::Network(e.to_string()))?;
        Ok((bytes.to_vec(), hdrs))
    }

    #[instrument(skip(self))]
    pub async fn fetch_feed(&self, url: &str) -> Result<ParsedFeed> {
        let (bytes, _hdrs) = self.fetch_bytes(url).await?;
        parser::parse(bytes.as_slice()).map_err(|e| Error::Parse(e.to_string()))
    }
}

impl FeedFetcher for HttpFetcher {
    fn client(&self) -> &Client {
        &self.client
    }
    fn options(&self) -> &FetchOptions {
        &self.options
    }
}
