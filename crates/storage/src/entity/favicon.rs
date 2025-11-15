use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "favicon")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub feed_id: Option<i64>,
    pub url: Option<String>,
    pub mime: Option<String>,
    pub data: Option<Vec<u8>>, // Optional for now; may become required in future
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::feed::Entity",
        from = "Column::FeedId",
        to = "super::feed::Column::Id"
    )]
    Feed,
}

impl Related<super::feed::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Feed.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
