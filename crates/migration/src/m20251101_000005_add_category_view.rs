use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Category::Table)
                    .add_column_if_not_exists(ColumnDef::new(Category::View).string())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Category::Table)
                    .drop_column(Category::View)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Category {
    Table,
    View,
}

