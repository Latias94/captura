use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .add_column(ColumnDef::new(Feed::IntegrationsJson).json_binary())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .drop_column(Feed::IntegrationsJson)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Feed {
    Table,
    IntegrationsJson,
}
