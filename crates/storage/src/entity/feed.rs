use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Text")]
pub enum FeedType {
    #[sea_orm(string_value = "rss")]
    Rss,
    #[sea_orm(string_value = "atom")]
    Atom,
    #[sea_orm(string_value = "json")]
    Json,
    #[sea_orm(string_value = "rule")]
    Rule,
    #[sea_orm(string_value = "hub")]
    Hub,
}

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "feed")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub category_id: Option<i64>,
    pub r#type: FeedType,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub feed_url: String,
    pub favicon_id: Option<i64>,
    pub rule_id: Option<i64>,
    pub rule_params_json: Option<Json>,

    // Fetch options
    pub user_agent: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub headers_json: Option<Json>,
    pub cookies: Option<String>,
    pub proxy_url: Option<String>,
    pub fetch_via_proxy: bool,
    pub disable_http2: bool,
    pub allow_invalid_certs: bool,
    pub request_timeout_ms: Option<i32>,

    // Scheduling & state
    pub checked_at: Option<DateTimeWithTimeZone>,
    pub next_run_at: Option<DateTimeWithTimeZone>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_status: Option<i32>,
    pub error_count: i32,
    pub last_error_message: Option<String>,
    pub disabled: bool,

    // Preferred view for this feed (articles/pictures/videos/...)
    pub view: Option<String>,

    // Rewriting & filtering rules (text-based for portability)
    pub scraper_rules: Option<String>,
    pub rewrite_rules: Option<String>,
    pub blocklist_rules: Option<String>,
    pub keeplist_rules: Option<String>,
    pub url_rewrite_rules: Option<String>,
    pub block_filter_entry_rules: Option<String>,
    pub keep_filter_entry_rules: Option<String>,
    pub integrations_json: Option<Json>,

    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::category::Entity",
        from = "Column::CategoryId",
        to = "super::category::Column::Id"
    )]
    Category,
    #[sea_orm(
        belongs_to = "super::rule::Entity",
        from = "Column::RuleId",
        to = "super::rule::Column::Id"
    )]
    Rule,
    #[sea_orm(has_many = "super::entry::Entity")]
    Entry,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::category::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Category.def()
    }
}

impl Related<super::rule::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Rule.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
