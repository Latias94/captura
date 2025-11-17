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
                    .add_column_if_not_exists(
                        ColumnDef::new(Feed::View)
                            .string()
                            .not_null()
                            .default("articles"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Feed::Table)
                    .drop_column(Feed::View)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Feed {
    Table,
    View,
}
