use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Add column favicon_id to feed
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .add_column(ColumnDef::new(Feed::FaviconId).big_integer())
                    .to_owned(),
            )
            .await?;

        // Create favicon table
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

        // Indexes
        manager
            .create_index(
                Index::create()
                    .name("idx_favicon_feed")
                    .table(Favicon::Table)
                    .col(Favicon::FeedId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Favicon::Table).to_owned())
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .drop_column(Feed::FaviconId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Feed {
    Table,
    Id,
    FaviconId,
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
