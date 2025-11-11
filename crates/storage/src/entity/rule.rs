use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub rule_id: String, // e.g. namespace.id
    pub version: Option<String>,
    pub namespace: Option<String>,
    pub description: Option<String>,
    pub yaml: String,
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
