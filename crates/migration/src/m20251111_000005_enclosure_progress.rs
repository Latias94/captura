use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Enclosure::Table)
                    .add_column(ColumnDef::new(Enclosure::MediaProgression).big_integer())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Enclosure::Table)
                    .drop_column(Enclosure::MediaProgression)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum Enclosure {
    Table,
    MediaProgression,
}
