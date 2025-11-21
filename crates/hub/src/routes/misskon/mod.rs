pub mod posts;
pub mod tag;
pub mod top;

use captura_common::{Error, Result};
use serde::Deserialize;

const ENDPOINT: &str = "https://misskon.com/wp-json/wp/v2";

#[derive(Debug, Deserialize)]
pub struct WpRendered {
    pub rendered: String,
}

#[derive(Debug, Deserialize)]
pub struct WpPost {
    pub title: WpRendered,
    pub link: String,
    pub date_gmt: Option<String>,
    pub content: WpRendered,
    #[serde(rename = "_embedded")]
    pub embedded: Option<WpEmbedded>,
}

#[derive(Debug, Deserialize)]
pub struct WpEmbedded {
    #[serde(rename = "wp:term")]
    pub terms: Option<Vec<Vec<WpTerm>>>,
}

#[derive(Debug, Deserialize)]
pub struct WpTerm {
    pub taxonomy: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug)]
pub struct MisskonPost {
    pub title: String,
    pub link: String,
    pub description: String,
    pub date_gmt: Option<String>,
    pub tags: Vec<String>,
}

pub async fn fetch_posts(query: &str) -> Result<Vec<MisskonPost>> {
    let mut url = format!("{}/posts", ENDPOINT);
    if !query.is_empty() {
        url.push('?');
        url.push_str(query);
        url.push('&');
    } else {
        url.push('?');
    }
    url.push_str("_embed=wp:term");

    let posts: Vec<WpPost> = crate::routes::util::get_json(&url).await?;

    let mut out = Vec::new();
    for p in posts {
        let title = p.title.rendered.clone();
        let link = p.link.clone();
        let description = p.content.rendered.clone();
        let mut tags = Vec::new();
        if let Some(embed) = &p.embedded {
            if let Some(groups) = &embed.terms {
                for group in groups {
                    for t in group {
                        if t.taxonomy.as_deref() == Some("post_tag") {
                            if let Some(name) = &t.name {
                                tags.push(name.clone());
                            }
                        }
                    }
                }
            }
        }
        out.push(MisskonPost {
            title,
            link,
            description,
            date_gmt: p.date_gmt.clone(),
            tags,
        });
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct WpTag {
    pub id: i64,
    pub name: String,
    pub link: String,
    pub description: String,
}

#[derive(Debug)]
pub struct MisskonTag {
    pub id: i64,
    pub name: String,
    pub link: String,
    pub description: String,
}

pub async fn fetch_tag(slug: &str) -> Result<MisskonTag> {
    let url = format!("{}/tags?slug={}", ENDPOINT, slug);
    let tags: Vec<WpTag> = crate::routes::util::get_json(&url).await?;
    let first = tags
        .into_iter()
        .next()
        .ok_or_else(|| Error::Parse(format!("misskon: invalid tag slug {}", slug)))?;
    Ok(MisskonTag {
        id: first.id,
        name: first.name,
        link: first.link,
        description: first.description,
    })
}
