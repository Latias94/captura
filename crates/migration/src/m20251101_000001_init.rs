use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // user
        manager
            .create_table(
                Table::create()
                    .table(User::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(User::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(User::Username)
                            .string_len(190)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(User::PasswordHash).string().not_null())
                    .col(ColumnDef::new(User::FeverKeyMd5).string())
                    .col(
                        ColumnDef::new(User::Role)
                            .string_len(16)
                            .not_null()
                            .default("user"),
                    )
                    .col(
                        ColumnDef::new(User::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // category
        manager
            .create_table(
                Table::create()
                    .table(Category::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Category::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Category::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Category::Name).string_len(190).not_null())
                    .col(
                        ColumnDef::new(Category::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_category_user")
                            .from(Category::Table, Category::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // rule
        manager
            .create_table(
                Table::create()
                    .table(Rule::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Rule::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Rule::RuleId)
                            .string_len(190)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Rule::Kind)
                            .string_len(16)
                            .not_null()
                            .default("dsl"),
                    )
                    .col(ColumnDef::new(Rule::Version).string_len(64))
                    .col(ColumnDef::new(Rule::Namespace).string_len(128))
                    .col(ColumnDef::new(Rule::Description).string())
                    .col(ColumnDef::new(Rule::SpecJson).json_binary())
                    .col(ColumnDef::new(Rule::HandlerTarget).string_len(190))
                    .col(ColumnDef::new(Rule::ExamplesJson).json_binary())
                    .col(ColumnDef::new(Rule::VerifiedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Rule::Maintainer).string_len(190))
                    .col(
                        ColumnDef::new(Rule::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Rule::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // feed
        manager
            .create_table(
                Table::create()
                    .table(Feed::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Feed::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Feed::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Feed::CategoryId).big_integer())
                    .col(ColumnDef::new(Feed::Type).string_len(16).not_null())
                    .col(ColumnDef::new(Feed::Title).string())
                    .col(ColumnDef::new(Feed::SiteUrl).string())
                    .col(ColumnDef::new(Feed::FeedUrl).string().not_null())
                    .col(ColumnDef::new(Feed::FaviconId).big_integer())
                    .col(ColumnDef::new(Feed::RuleId).big_integer())
                    .col(ColumnDef::new(Feed::RuleParamsJson).json_binary())
                    .col(ColumnDef::new(Feed::UserAgent).string())
                    .col(ColumnDef::new(Feed::Username).string())
                    .col(ColumnDef::new(Feed::Password).string())
                    .col(ColumnDef::new(Feed::HeadersJson).json_binary())
                    .col(ColumnDef::new(Feed::Cookies).text())
                    .col(ColumnDef::new(Feed::ProxyUrl).string())
                    .col(
                        ColumnDef::new(Feed::FetchViaProxy)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Feed::DisableHttp2)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Feed::AllowInvalidCerts)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Feed::RequestTimeoutMs).integer())
                    .col(ColumnDef::new(Feed::CheckedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Feed::NextRunAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(Feed::Etag).string())
                    .col(ColumnDef::new(Feed::LastModified).string())
                    .col(ColumnDef::new(Feed::LastStatus).integer())
                    .col(ColumnDef::new(Feed::LastErrorMessage).text())
                    .col(
                        ColumnDef::new(Feed::ErrorCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Feed::Disabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Feed::ScraperRules).text())
                    .col(ColumnDef::new(Feed::RewriteRules).text())
                    .col(ColumnDef::new(Feed::BlocklistRules).text())
                    .col(ColumnDef::new(Feed::KeeplistRules).text())
                    .col(ColumnDef::new(Feed::UrlRewriteRules).text())
                    .col(ColumnDef::new(Feed::BlockFilterEntryRules).text())
                    .col(ColumnDef::new(Feed::KeepFilterEntryRules).text())
                    .col(ColumnDef::new(Feed::IntegrationsJson).json_binary())
                    .col(
                        ColumnDef::new(Feed::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Feed::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    // indexes created after table (SQLite compatibility)
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_feed_user")
                            .from(Feed::Table, Feed::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_feed_category")
                            .from(Feed::Table, Feed::CategoryId)
                            .to(Category::Table, Category::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_feed_rule")
                            .from(Feed::Table, Feed::RuleId)
                            .to(Rule::Table, Rule::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // favicon
        manager
            .create_table(
                Table::create()
                    .table(Favicon::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Favicon::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Favicon::FeedId).big_integer())
                    .col(ColumnDef::new(Favicon::Url).string())
                    .col(ColumnDef::new(Favicon::Mime).string_len(64))
                    .col(ColumnDef::new(Favicon::Data).binary())
                    .col(
                        ColumnDef::new(Favicon::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Favicon::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_favicon_feed")
                            .from(Favicon::Table, Favicon::FeedId)
                            .to(Feed::Table, Feed::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // entry
        manager
            .create_table(
                Table::create()
                    .table(Entry::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Entry::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Entry::FeedId).big_integer().not_null())
                    .col(ColumnDef::new(Entry::Guid).string())
                    .col(ColumnDef::new(Entry::Url).string())
                    .col(ColumnDef::new(Entry::Title).string())
                    .col(ColumnDef::new(Entry::Summary).text())
                    .col(ColumnDef::new(Entry::ContentHtml).text())
                    .col(ColumnDef::new(Entry::Author).string())
                    .col(ColumnDef::new(Entry::PublishedAt).timestamp_with_time_zone())
                    .col(
                        ColumnDef::new(Entry::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Entry::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Entry::Hash).string_len(64))
                    .col(
                        ColumnDef::new(Entry::IsRead)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Entry::IsStarred)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Entry::ExtrasJson).json_binary())
                    // indexes created after table (SQLite compatibility)
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entry_feed")
                            .from(Entry::Table, Entry::FeedId)
                            .to(Feed::Table, Feed::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // enclosure
        manager
            .create_table(
                Table::create()
                    .table(Enclosure::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Enclosure::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Enclosure::EntryId).big_integer().not_null())
                    .col(ColumnDef::new(Enclosure::Url).string().not_null())
                    .col(ColumnDef::new(Enclosure::Mime).string())
                    .col(ColumnDef::new(Enclosure::Length).big_integer())
                    .col(ColumnDef::new(Enclosure::Kind).string_len(16))
                    .col(ColumnDef::new(Enclosure::MediaProgression).big_integer())
                    // indexes created after table (SQLite compatibility)
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_enclosure_entry")
                            .from(Enclosure::Table, Enclosure::EntryId)
                            .to(Entry::Table, Entry::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // label
        manager
            .create_table(
                Table::create()
                    .table(Label::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Label::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Label::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Label::Name).string_len(190).not_null())
                    .col(ColumnDef::new(Label::Color).string_len(16))
                    .col(
                        ColumnDef::new(Label::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    // indexes created after table (SQLite compatibility)
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_label_user")
                            .from(Label::Table, Label::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // entry_label
        manager
            .create_table(
                Table::create()
                    .table(EntryLabel::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EntryLabel::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(EntryLabel::EntryId).big_integer().not_null())
                    .col(ColumnDef::new(EntryLabel::LabelId).big_integer().not_null())
                    // indexes created after table (SQLite compatibility)
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entrylabel_entry")
                            .from(EntryLabel::Table, EntryLabel::EntryId)
                            .to(Entry::Table, Entry::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_entrylabel_label")
                            .from(EntryLabel::Table, EntryLabel::LabelId)
                            .to(Label::Table, Label::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // job
        manager
            .create_table(
                Table::create()
                    .table(Job::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Job::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Job::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Job::FeedId).big_integer())
                    .col(ColumnDef::new(Job::RuleId).big_integer())
                    .col(ColumnDef::new(Job::JobType).string_len(24).not_null())
                    .col(ColumnDef::new(Job::Status).string_len(16).not_null())
                    .col(
                        ColumnDef::new(Job::Priority)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Job::RunAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Job::Attempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(Job::LastError).text())
                    .col(ColumnDef::new(Job::PayloadJson).json_binary())
                    .col(
                        ColumnDef::new(Job::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Job::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    // indexes created after table (SQLite compatibility)
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_job_user")
                            .from(Job::Table, Job::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_job_feed")
                            .from(Job::Table, Job::FeedId)
                            .to(Feed::Table, Feed::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_job_rule")
                            .from(Job::Table, Job::RuleId)
                            .to(Rule::Table, Rule::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // api_token
        manager
            .create_table(
                Table::create()
                    .table(ApiToken::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ApiToken::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ApiToken::UserId).big_integer().not_null())
                    .col(ColumnDef::new(ApiToken::Name).string_len(190))
                    .col(ColumnDef::new(ApiToken::TokenHash).string().not_null())
                    .col(ColumnDef::new(ApiToken::TokenPlain).string())
                    .col(
                        ColumnDef::new(ApiToken::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ApiToken::LastUsedAt).timestamp_with_time_zone())
                    .col(ColumnDef::new(ApiToken::ExpiresAt).timestamp_with_time_zone())
                    // indexes created after table (SQLite compatibility)
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_api_token_user")
                            .from(ApiToken::Table, ApiToken::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // integration
        manager
            .create_table(
                Table::create()
                    .table(Integration::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Integration::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Integration::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Integration::Kind).string_len(64).not_null())
                    .col(ColumnDef::new(Integration::ConfigJson).json_binary())
                    .col(
                        ColumnDef::new(Integration::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Integration::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Integration::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_integration_user")
                            .from(Integration::Table, Integration::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // webhook
        manager
            .create_table(
                Table::create()
                    .table(Webhook::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Webhook::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Webhook::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Webhook::Url).string().not_null())
                    .col(ColumnDef::new(Webhook::Secret).string().not_null())
                    .col(ColumnDef::new(Webhook::Events).string())
                    .col(
                        ColumnDef::new(Webhook::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Webhook::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Webhook::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_webhook_user")
                            .from(Webhook::Table, Webhook::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // user_pref
        manager
            .create_table(
                Table::create()
                    .table(UserPref::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserPref::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UserPref::UserId).big_integer().not_null())
                    .col(ColumnDef::new(UserPref::Key).string_len(64).not_null())
                    .col(ColumnDef::new(UserPref::ValueJson).json_binary())
                    .col(
                        ColumnDef::new(UserPref::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserPref::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_userpref_user")
                            .from(UserPref::Table, UserPref::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Feed unique (user_id, feed_url)
        manager
            .create_index(
                Index::create()
                    .name("idx_feed_user_url")
                    .table(Feed::Table)
                    .col(Feed::UserId)
                    .col(Feed::FeedUrl)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Entry indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_entry_feed_guid")
                    .table(Entry::Table)
                    .col(Entry::FeedId)
                    .col(Entry::Guid)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_entry_feed_published")
                    .table(Entry::Table)
                    .col(Entry::FeedId)
                    .col(Entry::PublishedAt)
                    .to_owned(),
            )
            .await?;

        // Enclosure index
        manager
            .create_index(
                Index::create()
                    .name("idx_enclosure_entry")
                    .table(Enclosure::Table)
                    .col(Enclosure::EntryId)
                    .to_owned(),
            )
            .await?;

        // Favicon index
        manager
            .create_index(
                Index::create()
                    .name("idx_favicon_feed")
                    .table(Favicon::Table)
                    .col(Favicon::FeedId)
                    .to_owned(),
            )
            .await?;

        // Label unique index
        manager
            .create_index(
                Index::create()
                    .name("idx_label_user_name")
                    .table(Label::Table)
                    .col(Label::UserId)
                    .col(Label::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // EntryLabel unique index
        manager
            .create_index(
                Index::create()
                    .name("idx_entry_label")
                    .table(EntryLabel::Table)
                    .col(EntryLabel::EntryId)
                    .col(EntryLabel::LabelId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Feed/Entry unique indexes were created earlier; avoid duplicate definitions here.

        // Job index
        manager
            .create_index(
                Index::create()
                    .name("idx_job_status_runat")
                    .table(Job::Table)
                    .col(Job::Status)
                    .col(Job::RunAt)
                    .to_owned(),
            )
            .await?;

        // ApiToken index
        manager
            .create_index(
                Index::create()
                    .name("idx_api_token_user")
                    .table(ApiToken::Table)
                    .col(ApiToken::UserId)
                    .to_owned(),
            )
            .await?;

        // ApiToken unique by token hash
        manager
            .create_index(
                Index::create()
                    .name("idx_api_token_hash_unique")
                    .table(ApiToken::Table)
                    .col(ApiToken::TokenHash)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // UserPref unique index
        manager
            .create_index(
                Index::create()
                    .name("idx_userpref_user_key")
                    .table(UserPref::Table)
                    .col(UserPref::UserId)
                    .col(UserPref::Key)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Postgres-only: add tsv column/index and trigger for full-text search on entry table
        let db = manager.get_connection();
        if db.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            let stmts = [
                // Column and index
                "ALTER TABLE entry ADD COLUMN IF NOT EXISTS tsv tsvector;",
                // Pre-fill, limiting each segment to 500k characters and setting weights
                r#"
                UPDATE entry SET tsv =
                    setweight(to_tsvector('simple', left(coalesce(title,''), 500000)), 'A') ||
                    setweight(to_tsvector('simple', left(coalesce(summary,''), 500000)), 'B') ||
                    setweight(to_tsvector('simple', left(coalesce(content_html,''), 500000)), 'C');
                "#,
                "CREATE INDEX IF NOT EXISTS idx_entry_tsv ON entry USING GIN (tsv);",
                // Trigger function
                r#"
                CREATE OR REPLACE FUNCTION entry_tsv_update() RETURNS trigger AS $$
                BEGIN
                    NEW.tsv :=
                        setweight(to_tsvector('simple', left(coalesce(NEW.title,''), 500000)), 'A') ||
                        setweight(to_tsvector('simple', left(coalesce(NEW.summary,''), 500000)), 'B') ||
                        setweight(to_tsvector('simple', left(coalesce(NEW.content_html,''), 500000)), 'C');
                    RETURN NEW;
                END
                $$ LANGUAGE plpgsql;
                "#,
                // Trigger
                "DROP TRIGGER IF EXISTS entry_tsv_update ON entry;",
                "CREATE TRIGGER entry_tsv_update BEFORE INSERT OR UPDATE OF title, summary, content_html ON entry FOR EACH ROW EXECUTE FUNCTION entry_tsv_update();",
            ];
            for sql in stmts {
                db.execute_unprepared(sql).await?;
            }
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserPref::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Webhook::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Integration::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ApiToken::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Job::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(EntryLabel::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Label::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Enclosure::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Favicon::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Entry::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Feed::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Rule::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Category::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(User::Table).to_owned())
            .await?;
        // Postgres-only: drop FTS trigger/index/column if present
        let db = manager.get_connection();
        if db.get_database_backend() == sea_orm::DatabaseBackend::Postgres {
            let stmts = [
                "DROP TRIGGER IF EXISTS entry_tsv_update ON entry;",
                "DROP FUNCTION IF EXISTS entry_tsv_update();",
                "DROP INDEX IF EXISTS idx_entry_tsv;",
                "ALTER TABLE entry DROP COLUMN IF EXISTS tsv;",
            ];
            for sql in stmts {
                db.execute_unprepared(sql).await?;
            }
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
    Username,
    PasswordHash,
    FeverKeyMd5,
    Role,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Category {
    Table,
    Id,
    UserId,
    Name,
    CreatedAt,
}

#[derive(DeriveIden)]
#[allow(clippy::enum_variant_names)]
enum Rule {
    Table,
    Id,
    RuleId,
    Kind,
    Version,
    Namespace,
    Description,
    SpecJson,
    HandlerTarget,
    ExamplesJson,
    VerifiedAt,
    Maintainer,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
#[allow(clippy::enum_variant_names)]
enum Feed {
    Table,
    Id,
    UserId,
    CategoryId,
    Type,
    Title,
    SiteUrl,
    FeedUrl,
    FaviconId,
    RuleId,
    RuleParamsJson,
    UserAgent,
    Username,
    Password,
    HeadersJson,
    Cookies,
    ProxyUrl,
    FetchViaProxy,
    DisableHttp2,
    AllowInvalidCerts,
    RequestTimeoutMs,
    CheckedAt,
    NextRunAt,
    Etag,
    LastModified,
    LastStatus,
    LastErrorMessage,
    ErrorCount,
    Disabled,
    ScraperRules,
    RewriteRules,
    BlocklistRules,
    KeeplistRules,
    UrlRewriteRules,
    BlockFilterEntryRules,
    KeepFilterEntryRules,
    IntegrationsJson,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Entry {
    Table,
    Id,
    FeedId,
    Guid,
    Url,
    Title,
    Summary,
    ContentHtml,
    Author,
    PublishedAt,
    CreatedAt,
    UpdatedAt,
    Hash,
    IsRead,
    IsStarred,
    ExtrasJson,
}

#[derive(DeriveIden)]
enum Enclosure {
    Table,
    Id,
    EntryId,
    Url,
    Mime,
    Length,
    Kind,
    MediaProgression,
}

#[derive(DeriveIden)]
enum Favicon {
    Table,
    Id,
    FeedId,
    Url,
    Mime,
    Data,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Label {
    Table,
    Id,
    UserId,
    Name,
    Color,
    CreatedAt,
}

#[derive(DeriveIden)]
enum EntryLabel {
    Table,
    Id,
    EntryId,
    LabelId,
}

#[derive(DeriveIden)]
#[allow(clippy::enum_variant_names)]
enum Job {
    Table,
    Id,
    UserId,
    FeedId,
    RuleId,
    JobType,
    Status,
    Priority,
    RunAt,
    Attempts,
    LastError,
    PayloadJson,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ApiToken {
    Table,
    Id,
    UserId,
    Name,
    TokenHash,
    TokenPlain,
    CreatedAt,
    LastUsedAt,
    ExpiresAt,
}

#[derive(DeriveIden)]
enum Integration {
    Table,
    Id,
    UserId,
    Kind,
    ConfigJson,
    Enabled,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Webhook {
    Table,
    Id,
    UserId,
    Url,
    Secret,
    Events,
    Enabled,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum UserPref {
    Table,
    Id,
    UserId,
    Key,
    ValueJson,
    CreatedAt,
    UpdatedAt,
}
