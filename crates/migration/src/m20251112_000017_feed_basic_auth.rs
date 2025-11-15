use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Sqlite does not support adding multiple columns in a single ALTER TABLE, so split into two statements
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .add_column(ColumnDef::new(Feed::Username).string())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .add_column(ColumnDef::new(Feed::Password).string())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .drop_column(Feed::Username)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .drop_column(Feed::Password)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Feed {
    Table,
    Username,
    Password,
}
