use captura_common::Result;
use captura_service::query::{list_entries_for_user, TimelineQuery, TimelineStatus};
use captura_storage::entity::{entry, feed, user};
use captura_types::EntryView;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, QueryOrder, Set};

#[tokio::test]
async fn timeline_query_filters_by_feed_view_and_status() -> Result<()> {
    let db = captura_testkit::setup_db().await;

    // Seed user.
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let u = user::ActiveModel {
        id: Default::default(),
        username: Set("timeline".into()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Two feeds for the same user, different views.
    let f_articles = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("articles feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/articles.xml".into()),
        favicon_id: Set(None),
        rule_id: Set(None),
        rule_params_json: Set(None),
        user_agent: Set(None),
        username: Set(None),
        password: Set(None),
        headers_json: Set(None),
        cookies: Set(None),
        proxy_url: Set(None),
        fetch_via_proxy: Set(false),
        disable_http2: Set(false),
        allow_invalid_certs: Set(false),
        request_timeout_ms: Set(None),
        checked_at: Set(None),
        next_run_at: Set(None),
        etag: Set(None),
        last_modified: Set(None),
        last_status: Set(None),
        error_count: Set(0),
        last_error_message: Set(None),
        disabled: Set(false),
        view: Set(Some(EntryView::Articles.to_db())),
        scraper_rules: Set(None),
        rewrite_rules: Set(None),
        blocklist_rules: Set(None),
        keeplist_rules: Set(None),
        url_rewrite_rules: Set(None),
        block_filter_entry_rules: Set(None),
        keep_filter_entry_rules: Set(None),
        integrations_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    let f_pictures = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("pictures feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/pictures.xml".into()),
        favicon_id: Set(None),
        rule_id: Set(None),
        rule_params_json: Set(None),
        user_agent: Set(None),
        username: Set(None),
        password: Set(None),
        headers_json: Set(None),
        cookies: Set(None),
        proxy_url: Set(None),
        fetch_via_proxy: Set(false),
        disable_http2: Set(false),
        allow_invalid_certs: Set(false),
        request_timeout_ms: Set(None),
        checked_at: Set(None),
        next_run_at: Set(None),
        etag: Set(None),
        last_modified: Set(None),
        last_status: Set(None),
        error_count: Set(0),
        last_error_message: Set(None),
        disabled: Set(false),
        view: Set(Some(EntryView::Pictures.to_db())),
        scraper_rules: Set(None),
        rewrite_rules: Set(None),
        blocklist_rules: Set(None),
        keeplist_rules: Set(None),
        url_rewrite_rules: Set(None),
        block_filter_entry_rules: Set(None),
        keep_filter_entry_rules: Set(None),
        integrations_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Seed entries: two unread, one read, mixed across feeds.
    async fn insert_entry(
        db: &sea_orm::DatabaseConnection,
        fid: i64,
        title: &str,
        is_read: bool,
        now: chrono::DateTime<FixedOffset>,
    ) {
        let am = entry::ActiveModel {
            id: Default::default(),
            feed_id: Set(fid),
            guid: Set(Some(format!("guid-{}-{}", fid, title))),
            url: Set(Some(format!("https://example.com/{}/{}", fid, title))),
            title: Set(Some(title.to_string())),
            summary: Set(None),
            content_html: Set(None),
            author: Set(None),
            published_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            hash: Set(None),
            is_read: Set(is_read),
            is_starred: Set(false),
            extras_json: Set(None),
        };
        am.insert(db).await.unwrap();
    }

    insert_entry(&db, f_articles.id, "a-unread", false, now).await;
    insert_entry(&db, f_articles.id, "a-read", true, now).await;
    insert_entry(&db, f_pictures.id, "p-unread", false, now).await;

    // 1) view=Articles + status=Unread should only see the unread entry from articles feed.
    let q_articles_unread = TimelineQuery {
        view: Some(EntryView::Articles),
        feed_ids: Vec::new(),
        category_ids: Vec::new(),
        label_ids: Vec::new(),
        status: Some(TimelineStatus::Unread),
        search: None,
        sort_by: Some("id".into()),
        sort_order: Some("asc".into()),
        limit: 100,
        offset: 0,
        before_id: None,
        after_id: None,
    };
    let res = list_entries_for_user(&db, u.id, &q_articles_unread).await?;
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].feed_id, f_articles.id);

    // 2) view=Pictures + status=Unread should only see the unread entry from pictures feed.
    let q_pictures_unread = TimelineQuery {
        view: Some(EntryView::Pictures),
        status: Some(TimelineStatus::Unread),
        ..q_articles_unread.clone()
    };
    let res = list_entries_for_user(&db, u.id, &q_pictures_unread).await?;
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].feed_id, f_pictures.id);

    // 3) view=All + status=Unread should see both unread entries.
    let q_all_unread = TimelineQuery {
        view: Some(EntryView::All),
        status: Some(TimelineStatus::Unread),
        ..q_articles_unread
    };
    let res = list_entries_for_user(&db, u.id, &q_all_unread).await?;
    assert_eq!(res.len(), 2);

    Ok(())
}

#[tokio::test]
async fn timeline_query_search_and_id_cursors() -> Result<()> {
    let db = captura_testkit::setup_db().await;

    // Seed user + single feed.
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());
    let u = user::ActiveModel {
        id: Default::default(),
        username: Set("search_user".into()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let f = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("search feed".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/search.xml".into()),
        favicon_id: Set(None),
        rule_id: Set(None),
        rule_params_json: Set(None),
        user_agent: Set(None),
        username: Set(None),
        password: Set(None),
        headers_json: Set(None),
        cookies: Set(None),
        proxy_url: Set(None),
        fetch_via_proxy: Set(false),
        disable_http2: Set(false),
        allow_invalid_certs: Set(false),
        request_timeout_ms: Set(None),
        checked_at: Set(None),
        next_run_at: Set(None),
        etag: Set(None),
        last_modified: Set(None),
        last_status: Set(None),
        error_count: Set(0),
        last_error_message: Set(None),
        disabled: Set(false),
        view: Set(Some(EntryView::Articles.to_db())),
        scraper_rules: Set(None),
        rewrite_rules: Set(None),
        blocklist_rules: Set(None),
        keeplist_rules: Set(None),
        url_rewrite_rules: Set(None),
        block_filter_entry_rules: Set(None),
        keep_filter_entry_rules: Set(None),
        integrations_json: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Insert three entries with different titles.
    let titles = ["hello world", "rust timeline", "another hello"];
    for t in titles.iter() {
        let am = entry::ActiveModel {
            id: Default::default(),
            feed_id: Set(f.id),
            guid: Set(Some(format!("guid-{}", t))),
            url: Set(Some(format!("https://example.com/{}", t.replace(' ', "_")))),
            title: Set(Some(t.to_string())),
            summary: Set(None),
            content_html: Set(None),
            author: Set(None),
            published_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            hash: Set(None),
            is_read: Set(false),
            is_starred: Set(false),
            extras_json: Set(None),
        };
        am.insert(&db).await.unwrap();
    }

    // Fetch all entries to know their ids (ordered by id).
    let all_ids: Vec<i64> = entry::Entity::find()
        .order_by_asc(entry::Column::Id)
        .all(&db)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(all_ids.len(), 3);

    // 1) Search for "hello" should match titles containing "hello" (2 entries).
    let q_search = TimelineQuery {
        view: Some(EntryView::Articles),
        feed_ids: vec![f.id],
        category_ids: Vec::new(),
        label_ids: Vec::new(),
        status: None,
        search: Some("hello".into()),
        sort_by: Some("id".into()),
        sort_order: Some("asc".into()),
        limit: 10,
        offset: 0,
        before_id: None,
        after_id: None,
    };
    let res = list_entries_for_user(&db, u.id, &q_search).await?;
    let titles_found: Vec<String> = res
        .iter()
        .map(|e| e.title.clone().unwrap_or_default())
        .collect();
    assert_eq!(titles_found.len(), 2);
    assert!(titles_found.iter().all(|t| t.contains("hello")));

    // 2) before_id cursor should exclude entries with id >= before_id.
    let q_before = TimelineQuery {
        before_id: Some(all_ids[2]),
        ..q_search.clone()
    };
    let res = list_entries_for_user(&db, u.id, &q_before).await?;
    assert!(res.iter().all(|e| e.id < all_ids[2]));

    // 3) after_id cursor should exclude entries with id <= after_id.
    let q_after = TimelineQuery {
        before_id: None,
        after_id: Some(all_ids[0]),
        ..q_search
    };
    let res = list_entries_for_user(&db, u.id, &q_after).await?;
    assert!(res.iter().all(|e| e.id > all_ids[0]));

    Ok(())
}
