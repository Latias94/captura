use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub rule_id: String, // e.g. namespace.id
    /// Logical implementation kind for this rule: "dsl" | "handler".
    pub kind: String,
    pub version: Option<String>,
    pub namespace: Option<String>,
    pub description: Option<String>,
    /// Parsed v1 rule spec JSON; for kind="dsl" this is the primary source of truth.
    pub spec_json: Option<Json>,
    /// Optional handler target identifier (e.g. "hub:bilibili/hot-search") for kind="handler".
    pub handler_target: Option<String>,
    pub examples_json: Option<Json>,
    pub verified_at: Option<DateTimeWithTimeZone>,
    pub maintainer: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::feed::Entity")]
    Feed,
}

impl ActiveModelBehavior for ActiveModel {}
