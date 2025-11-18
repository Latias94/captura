use captura_common::Result;
use captura_service::query::{
    category_unread_counters_for_user, feed_counters_for_user, mark_entries_read_for_labels,
    mark_entries_read_for_user,
};
use captura_storage::entity::{category, entry, entry_label, feed, label, user};
use captura_types::EntryView;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

/// Verify feed-level and category-level counters respect user scoping and read/unread status.
#[tokio::test]
async fn feed_and_category_counters_for_user() -> Result<()> {
    let db = captura_testkit::setup_db().await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    // Two users.
    let u1 = user::ActiveModel {
        id: Default::default(),
        username: Set("u1".into()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let u2 = user::ActiveModel {
        id: Default::default(),
        username: Set("u2".into()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Categories for u1.
    let c1 = category::ActiveModel {
        id: Default::default(),
        user_id: Set(u1.id),
        name: Set("cat1".into()),
        view: Set(Some(EntryView::Articles.to_db())),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let c2 = category::ActiveModel {
        id: Default::default(),
        user_id: Set(u1.id),
        name: Set("cat2".into()),
        view: Set(Some(EntryView::Pictures.to_db())),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    // Feeds: f1 in cat1, f2 uncategorized, f3 for user2 (should be ignored).
    let f1 = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u1.id),
        category_id: Set(Some(c1.id)),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("f1".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/f1.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let f2 = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u1.id),
        category_id: Set(None),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("f2".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/f2.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let _f3_other_user = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u2.id),
        category_id: Set(Some(c2.id)),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("f3".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/f3.xml".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Entries for u1: f1 has 2 unread, 1 read; f2 has 1 unread.
    let make_entry = |fid, is_read| entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(fid),
        // Use NULL guid to avoid unique constraint on (feed_id, guid).
        guid: Set(None),
        url: Set(Some("https://example.com/entry".into())),
        title: Set(Some("t".into())),
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
    make_entry(f1.id, false).insert(&db).await.unwrap();
    make_entry(f1.id, false).insert(&db).await.unwrap();
    make_entry(f1.id, true).insert(&db).await.unwrap();
    make_entry(f2.id, false).insert(&db).await.unwrap();

    // For u1: feed counters should only include f1/f2.
    let (reads, unreads) = feed_counters_for_user(&db, u1.id).await?;
    assert_eq!(reads.get(&f1.id).copied().unwrap_or(0), 1);
    assert_eq!(unreads.get(&f1.id).copied().unwrap_or(0), 2);
    assert_eq!(reads.get(&f2.id).copied().unwrap_or(0), 0);
    assert_eq!(unreads.get(&f2.id).copied().unwrap_or(0), 1);
    // No counters for other user's feed.
    assert!(reads.len() <= 2 && unreads.len() <= 2);

    // Category unread counters for u1:
    let cat_map = category_unread_counters_for_user(&db, u1.id).await?;
    // cat1 should have 2 unread (from f1).
    assert_eq!(
        cat_map.get(&Some(c1.id)).copied().unwrap_or(0),
        2,
        "cat1 unread mismatch"
    );
    // uncategorized (None) should see 1 unread (from f2).
    assert_eq!(
        cat_map.get(&None).copied().unwrap_or(0),
        1,
        "uncategorized unread mismatch"
    );

    Ok(())
}

/// Verify mark_entries_read_for_user scopes by feed/category/view, and that mark_entries_read_for_labels uses labels.
#[tokio::test]
async fn mark_entries_read_scopes_and_labels() -> Result<()> {
    let db = captura_testkit::setup_db().await;
    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    let u = user::ActiveModel {
        id: Default::default(),
        username: Set("mark_user".into()),
        password_hash: Set("h".into()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Two categories + two feeds with different views.
    let c_articles = category::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        name: Set("articles".into()),
        view: Set(Some(EntryView::Articles.to_db())),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let c_pictures = category::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        name: Set("pictures".into()),
        view: Set(Some(EntryView::Pictures.to_db())),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();

    let f_articles = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        category_id: Set(Some(c_articles.id)),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("fa".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/fa.xml".into()),
        view: Set(Some(EntryView::Articles.to_db())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();
    let f_pictures = feed::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        category_id: Set(Some(c_pictures.id)),
        r#type: Set(feed::FeedType::Rss),
        title: Set(Some("fp".into())),
        site_url: Set(None),
        feed_url: Set("https://example.com/fp.xml".into()),
        view: Set(Some(EntryView::Pictures.to_db())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Insert unread entries in both feeds.
    let make = |fid| entry::ActiveModel {
        id: Default::default(),
        feed_id: Set(fid),
        guid: Set(Some(format!("g-{}", fid))),
        url: Set(Some("https://example.com/e".into())),
        title: Set(Some("t".into())),
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
    let e1 = make(f_articles.id).insert(&db).await.unwrap();
    let e2 = make(f_pictures.id).insert(&db).await.unwrap();

    // 1) Scope by feed_id: only entries in f_articles should be marked read.
    let n = mark_entries_read_for_user(&db, u.id, Some(f_articles.id), None, None).await?;
    assert_eq!(n, 1);
    let ea = entry::Entity::find_by_id(e1.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let ep = entry::Entity::find_by_id(e2.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(ea.is_read && !ep.is_read, "feed_id scoping failed");

    // Reset is_read flags for next checks.
    let mut am1: entry::ActiveModel = ea.into();
    am1.is_read = Set(false);
    am1.update(&db).await.unwrap();
    let mut am2: entry::ActiveModel = ep.into();
    am2.is_read = Set(false);
    am2.update(&db).await.unwrap();

    // 2) Scope by category_id: only entries in c_pictures (f_pictures) should be marked read.
    let n = mark_entries_read_for_user(&db, u.id, None, Some(c_pictures.id), None).await?;
    assert_eq!(n, 1);
    let ea = entry::Entity::find_by_id(e1.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let ep = entry::Entity::find_by_id(e2.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(!ea.is_read && ep.is_read, "category_id scoping failed");

    // Reset again.
    let mut am1: entry::ActiveModel = ea.into();
    am1.is_read = Set(false);
    am1.update(&db).await.unwrap();
    let mut am2: entry::ActiveModel = ep.into();
    am2.is_read = Set(false);
    am2.update(&db).await.unwrap();

    // 3) Scope by view=Pictures: only entries from f_pictures should be marked read.
    let n = mark_entries_read_for_user(&db, u.id, None, None, Some(EntryView::Pictures)).await?;
    assert_eq!(n, 1);
    let ea = entry::Entity::find_by_id(e1.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let ep = entry::Entity::find_by_id(e2.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(!ea.is_read && ep.is_read, "view scoping failed");

    // 4) Scope by labels: create a label and attach only to e1, then mark by labels.
    let lbl = label::ActiveModel {
        id: Default::default(),
        user_id: Set(u.id),
        name: Set("tag1".into()),
        color: Set(None),
        created_at: Set(now),
    }
    .insert(&db)
    .await
    .unwrap();
    let _ = entry_label::ActiveModel {
        entry_id: Set(e1.id),
        label_id: Set(lbl.id),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    // Reset both to unread.
    let mut am1: entry::ActiveModel = entry::Entity::find_by_id(e1.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .into();
    am1.is_read = Set(false);
    am1.update(&db).await.unwrap();
    let mut am2: entry::ActiveModel = entry::Entity::find_by_id(e2.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap()
        .into();
    am2.is_read = Set(false);
    am2.update(&db).await.unwrap();

    let n = mark_entries_read_for_labels(&db, u.id, &[lbl.id]).await?;
    assert_eq!(n, 1);
    let ea = entry::Entity::find_by_id(e1.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let ep = entry::Entity::find_by_id(e2.id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert!(ea.is_read && !ep.is_read, "label scoping failed");

    Ok(())
}
