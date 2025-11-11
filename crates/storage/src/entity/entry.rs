use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "entry")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub feed_id: i64,
    pub guid: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub hash: Option<String>,
    pub is_read: bool,
    pub is_starred: bool,
    pub extras_json: Option<Json>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::feed::Entity",
        from = "Column::FeedId",
        to = "super::feed::Column::Id"
    )]
    Feed,
    #[sea_orm(has_many = "super::enclosure::Entity")]
    Enclosure,
}

impl ActiveModelBehavior for ActiveModel {}

impl Related<super::feed::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Feed.def()
    }
}
