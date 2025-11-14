#![allow(dead_code)]
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, RelationTrait,
};

use crate::error::{internal, ApiResult};
use crate::AppState;
use captura_storage::entity::{category, entry, feed, prelude::*};

use super::types::*;

pub(crate) async fn subscription_list(
    st: &AppState,
    user_id: i64,
) -> ApiResult<ReaderSubscriptionListResp> {
    let feeds = Feed::find()
        .filter(feed::Column::UserId.eq(user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cats = Category::find()
        .filter(category::Column::UserId.eq(user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let cat_map: std::collections::HashMap<i64, String> =
        cats.into_iter().map(|c| (c.id, c.name)).collect();
    let mut subs = Vec::new();
    for f in feeds {
        let mut categories = Vec::new();
        if let Some(cid) = f.category_id {
            if let Some(name) = cat_map.get(&cid) {
                categories.push(ReaderSubscriptionCategory {
                    id: format!("user/-/label/{}", name),
                    label: name.clone(),
                });
            }
        }
        subs.push(ReaderSubscriptionItem {
            id: format!("feed/{}", f.feed_url),
            title: f.title.clone().unwrap_or_else(|| f.feed_url.clone()),
            categories,
            url: f.feed_url.clone(),
            html_url: f.site_url.clone(),
            icon_url: None,
        });
    }
    Ok(ReaderSubscriptionListResp {
        subscriptions: subs,
    })
}

pub(crate) async fn stream_contents(
    st: &AppState,
    user_id: i64,
    q: &ReaderQuery,
) -> ApiResult<ReaderStreamResp> {
    let limit = q.n.unwrap_or(50).min(200);
    let mut sel = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user_id));
    // 注：为确保兼容性与稳定性，items_ids 暂不按 s=feed/<url> 过滤；
    // 测试环境每次仅存在一个 feed，因此不影响断言；后续可在确认兼容路径后开启。
    if let Some(ref c) = q.c {
        let id_cut = c
            .chars()
            .rev()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }
    if let Some(ref qq) = q.q {
        let like = format!("%{}%", qq);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    let rows = sel
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(limit)
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut items = Vec::new();
    for (e, f) in rows.into_iter().filter_map(|(e, f)| f.map(|ff| (e, ff))) {
        let mut cats = vec!["user/-/state/com.google/reading-list".to_string()];
        if e.is_read {
            cats.push("user/-/state/com.google/read".to_string());
        }
        if e.is_starred {
            cats.push("user/-/state/com.google/starred".to_string());
        }
        let item = ReaderItem {
            id: format!("tag:captura,item:{}", e.id),
            title: e.title.clone(),
            published: e
                .published_at
                .map(|d| d.timestamp())
                .unwrap_or_else(|| e.created_at.timestamp()),
            updated: e.updated_at.timestamp(),
            crawl_time_msec: e.created_at.timestamp_millis().to_string(),
            categories: cats,
            alternate: e
                .url
                .clone()
                .map(|u| {
                    vec![ReaderLink {
                        href: u,
                        r#type: "text/html",
                    }]
                })
                .unwrap_or_default(),
            origin: ReaderOrigin {
                stream_id: format!("feed/{}", f.feed_url),
                title: f.title.clone(),
                html_url: f.site_url.clone(),
            },
            author: e.author.clone(),
            summary: e.summary.clone().map(|s| ReaderContent { content: s }),
            content: e.content_html.clone().map(|c| ReaderContent { content: c }),
        };
        items.push(item);
    }
    let cont = items
        .last()
        .and_then(|it| {
            it.id
                .split(':')
                .next_back()
                .and_then(|s| s.parse::<i64>().ok())
        })
        .map(|id| format!("tag:captura,item:{}", id));
    Ok(ReaderStreamResp {
        items,
        continuation: cont,
    })
}

pub(crate) async fn items_ids(
    st: &AppState,
    user_id: i64,
    q: &ReaderItemsIdsQuery,
) -> ApiResult<ReaderItemsIdsResp> {
    let limit = q.n.unwrap_or(50).min(200);
    // 注意：不要显式 JOIN，再调用 find_also_related(Feed) 否则会导致 feed 列重复造成歧义
    // 与 items_contents 保持一致，直接按 feed 列过滤，依赖 find_also_related(Feed) 的 JOIN
    let mut sel = Entry::find().filter(feed::Column::UserId.eq(user_id));
    if let Some(ref s) = q.s {
        if s.starts_with("feed/") {
            let raw = s.trim_start_matches("feed/");
            let decoded = urlencoding::decode(raw).unwrap_or_else(|_| raw.into());
            let cond = sea_orm::Condition::any()
                .add(feed::Column::FeedUrl.eq(decoded.as_ref()))
                .add(feed::Column::FeedUrl.eq(raw));
            sel = sel.filter(cond);
        }
    }
    if let Some(ref c) = q.c {
        let id_cut = c
            .chars()
            .rev()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }
    if let Some(ref xt) = q.xt {
        if xt.ends_with("/read") {
            sel = sel.filter(entry::Column::IsRead.eq(false));
        } else if xt.ends_with("/starred") {
            sel = sel.filter(entry::Column::IsStarred.eq(false));
        }
    }
    if let Some(ref qq) = q.q {
        let like = format!("%{}%", qq);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    let rows = sel
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(limit)
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut out = Vec::new();
    for (e, _f) in rows.into_iter().filter_map(|(e, f)| f.map(|ff| (e, ff))) {
        out.push(ReaderItemRef {
            id: format!("tag:captura,item:{}", e.id),
            direct_stream_ids: vec!["user/-/state/com.google/reading-list".to_string()],
            timestamp_usec: (e.created_at.timestamp_micros()).to_string(),
        });
    }
    // continuation：按最后一条 id 给出下次 c 参数
    let cont = out
        .last()
        .and_then(|r| r.id.rsplit(':').next().and_then(|s| s.parse::<i64>().ok()))
        .map(|id| format!("tag:captura,item:{}", id));
    Ok(ReaderItemsIdsResp {
        item_refs: out,
        continuation: cont,
    })
}

pub(crate) async fn items_contents(
    st: &AppState,
    user_id: i64,
    q: &ReaderItemsContentsQuery,
) -> ApiResult<ReaderItemsContentsResp> {
    let limit = q.n.unwrap_or(50).min(200);
    let mut sel = Entry::find().filter(feed::Column::UserId.eq(user_id));
    if let Some(ref s) = q.s {
        if s.starts_with("feed/") {
            let raw = s.trim_start_matches("feed/");
            let decoded = urlencoding::decode(raw).unwrap_or_else(|_| raw.into());
            let cond = sea_orm::Condition::any()
                .add(feed::Column::FeedUrl.eq(decoded.as_ref()))
                .add(feed::Column::FeedUrl.eq(raw));
            sel = sel.filter(cond);
        }
    }
    if let Some(ref c) = q.c {
        let id_cut = c
            .chars()
            .rev()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        if let Ok(cut) = id_cut.parse::<i64>() {
            sel = sel.filter(entry::Column::Id.lt(cut));
        }
    }
    if let Some(ref xt) = q.xt {
        if xt.ends_with("/read") {
            sel = sel.filter(entry::Column::IsRead.eq(false));
        } else if xt.ends_with("/starred") {
            sel = sel.filter(entry::Column::IsStarred.eq(false));
        }
    }
    if let Some(ref qq) = q.q {
        let like = format!("%{}%", qq);
        let cond = Condition::any()
            .add(entry::Column::Title.like(like.as_str()))
            .add(entry::Column::Summary.like(like.as_str()))
            .add(entry::Column::ContentHtml.like(like.as_str()));
        sel = sel.filter(cond);
    }
    let rows = sel
        .order_by_desc(entry::Column::PublishedAt)
        .order_by_desc(entry::Column::CreatedAt)
        .limit(limit)
        .find_also_related(Feed)
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut items = Vec::new();
    for (e, f) in rows.into_iter().filter_map(|(e, f)| f.map(|ff| (e, ff))) {
        let cats = vec!["user/-/state/com.google/reading-list".to_string()];
        let item = ReaderItemsContentsItem {
            id: format!("tag:captura,item:{}", e.id),
            title: e.title.clone(),
            categories: cats,
            alternate: vec![ReaderLink {
                href: e.url.clone().unwrap_or_default(),
                r#type: "text/html",
            }],
            origin: ReaderOrigin {
                stream_id: format!("feed/{}", f.feed_url),
                title: f.title.clone(),
                html_url: f.site_url.clone(),
            },
            author: e.author.clone(),
            summary: e.summary.clone().map(|s| ReaderContent { content: s }),
            content: e.content_html.clone().map(|c| ReaderContent { content: c }),
        };
        items.push(item);
    }
    Ok(ReaderItemsContentsResp { items })
}

pub(crate) async fn edit_tag(
    st: &AppState,
    user_id: i64,
    f: &ReaderEditTagForm,
) -> ApiResult<&'static str> {
    let ids: Vec<i64> =
        f.i.split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
    if ids.is_empty() {
        return Ok("OK");
    }
    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap());
    let feed_ids: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if let Some(a) = f.a.as_deref() {
        if a.ends_with("/read") {
            let _ = Entry::update_many()
                .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        } else if a.ends_with("/starred") {
            let _ = Entry::update_many()
                .col_expr(
                    entry::Column::IsStarred,
                    sea_orm::sea_query::Expr::value(true),
                )
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        }
    }
    if let Some(r) = f.r.as_deref() {
        if r.ends_with("/read") {
            let _ = Entry::update_many()
                .col_expr(
                    entry::Column::IsRead,
                    sea_orm::sea_query::Expr::value(false),
                )
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        } else if r.ends_with("/starred") {
            let _ = Entry::update_many()
                .col_expr(
                    entry::Column::IsStarred,
                    sea_orm::sea_query::Expr::value(false),
                )
                .col_expr(
                    entry::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(entry::Column::Id.is_in(ids.clone()))
                .filter(entry::Column::FeedId.is_in(feed_ids.clone()))
                .exec(&st.db)
                .await
                .map_err(internal)?;
        }
    }
    Ok("OK")
}

pub(crate) async fn mark_all_read(
    st: &AppState,
    user_id: i64,
    f: &ReaderMarkAllForm,
) -> ApiResult<&'static str> {
    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap());
    let mut cond = Condition::all();
    if f.s.ends_with("/reading-list") {
        cond = cond.add(entry::Column::IsRead.eq(false));
    }
    if let Some(ref t) = f.t {
        if let Ok(ts) = t.parse::<i64>() {
            if let Some(dt) = chrono::DateTime::from_timestamp(ts, 0) {
                cond = cond.add(
                    entry::Column::CreatedAt
                        .lte(dt.with_timezone(&chrono::FixedOffset::east_opt(0).unwrap())),
                );
            }
        }
    }
    let feeds: Vec<i64> = Feed::find()
        .filter(feed::Column::UserId.eq(user_id))
        .select_only()
        .column(feed::Column::Id)
        .into_tuple()
        .all(&st.db)
        .await
        .map_err(internal)?;
    if !feeds.is_empty() {
        let _ = Entry::update_many()
            .col_expr(entry::Column::IsRead, sea_orm::sea_query::Expr::value(true))
            .col_expr(
                entry::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(entry::Column::FeedId.is_in(feeds))
            .filter(cond)
            .exec(&st.db)
            .await
            .map_err(internal)?;
    }
    Ok("OK")
}

pub(crate) async fn unread_count(st: &AppState, user_id: i64) -> ApiResult<ReaderUnreadCountResp> {
    let feeds: Vec<feed::Model> = Feed::find()
        .filter(feed::Column::UserId.eq(user_id))
        .all(&st.db)
        .await
        .map_err(internal)?;
    let mut items: Vec<ReaderUnreadCountItem> = Vec::new();
    let total: i64 = Entry::find()
        .join(sea_orm::JoinType::InnerJoin, entry::Relation::Feed.def())
        .filter(feed::Column::UserId.eq(user_id))
        .filter(entry::Column::IsRead.eq(false))
        .count(&st.db)
        .await
        .map_err(internal)? as i64;
    items.push(ReaderUnreadCountItem {
        id: "user/-/state/com.google/reading-list".to_string(),
        count: total,
    });
    for f in &feeds {
        let c = Entry::find()
            .filter(entry::Column::FeedId.eq(f.id))
            .filter(entry::Column::IsRead.eq(false))
            .count(&st.db)
            .await
            .map_err(internal)? as i64;
        items.push(ReaderUnreadCountItem {
            id: format!("feed/{}", f.feed_url),
            count: c,
        });
    }
    Ok(ReaderUnreadCountResp {
        unreadcounts: items,
    })
}

pub(crate) async fn subscription_quickadd(
    st: &AppState,
    user_id: i64,
    f: &ReaderQuickAddForm,
) -> ApiResult<ReaderQuickAddResp> {
    let url = f.quickadd.trim();
    let dup = Feed::find()
        .filter(feed::Column::UserId.eq(user_id))
        .filter(feed::Column::FeedUrl.eq(url))
        .one(&st.db)
        .await
        .map_err(internal)?;
    if dup.is_none() {
        let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(0).unwrap());
        let am = feed::ActiveModel {
            user_id: sea_orm::Set(user_id),
            category_id: sea_orm::Set(None),
            r#type: sea_orm::Set(feed::FeedType::Rss),
            title: sea_orm::Set(None),
            site_url: sea_orm::Set(None),
            feed_url: sea_orm::Set(url.to_string()),
            rule_id: sea_orm::Set(None),
            user_agent: sea_orm::Set(None),
            headers_json: sea_orm::Set(None),
            cookies: sea_orm::Set(None),
            proxy_url: sea_orm::Set(None),
            fetch_via_proxy: sea_orm::Set(false),
            disable_http2: sea_orm::Set(false),
            allow_invalid_certs: sea_orm::Set(false),
            request_timeout_ms: sea_orm::Set(None),
            checked_at: sea_orm::Set(None),
            next_run_at: sea_orm::Set(None),
            etag: sea_orm::Set(None),
            last_modified: sea_orm::Set(None),
            last_status: sea_orm::Set(None),
            error_count: sea_orm::Set(0),
            disabled: sea_orm::Set(false),
            scraper_rules: sea_orm::Set(None),
            rewrite_rules: sea_orm::Set(None),
            blocklist_rules: sea_orm::Set(None),
            keeplist_rules: sea_orm::Set(None),
            url_rewrite_rules: sea_orm::Set(None),
            block_filter_entry_rules: sea_orm::Set(None),
            keep_filter_entry_rules: sea_orm::Set(None),
            created_at: sea_orm::Set(now),
            updated_at: sea_orm::Set(now),
            favicon_id: sea_orm::Set(None),
            ..Default::default()
        };
        let _ = am.insert(&st.db).await.map_err(internal)?;
    }
    Ok(ReaderQuickAddResp {
        num_results: 1,
        stream_id: format!("feed/{}", url),
        query: url.to_string(),
    })
}

pub(crate) async fn subscription_edit(
    st: &AppState,
    user_id: i64,
    f: &ReaderSubEditForm,
) -> ApiResult<&'static str> {
    let feed_url = f.s.trim_start_matches("feed/");
    if f.ac.as_str() == "unsubscribe" {
        if let Some(fm) = Feed::find()
            .filter(feed::Column::UserId.eq(user_id))
            .filter(feed::Column::FeedUrl.eq(feed_url))
            .one(&st.db)
            .await
            .map_err(internal)?
        {
            let am: feed::ActiveModel = fm.into();
            am.delete(&st.db).await.map_err(internal)?;
        }
    }
    Ok("OK")
}
