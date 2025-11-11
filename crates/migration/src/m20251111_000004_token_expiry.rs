use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ApiToken::Table)
                    .add_column(ColumnDef::new(ApiToken::ExpiresAt).timestamp_with_time_zone())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ApiToken::Table)
                    .drop_column(ApiToken::ExpiresAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ApiToken {
    Table,
    Id,
    UserId,
    Name,
    TokenHash,
    CreatedAt,
    LastUsedAt,
    ExpiresAt,
}

