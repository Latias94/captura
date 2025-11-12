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
                    .add_column(ColumnDef::new(ApiToken::TokenPlain).string())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ApiToken::Table)
                    .drop_column(ApiToken::TokenPlain)
                    .to_owned(),
            )
            .await
    }
}

#[allow(dead_code)]
#[derive(DeriveIden)]
enum ApiToken {
    Table,
    Id,
    UserId,
    Name,
    TokenHash,
    TokenPlain,
    CreatedAt,
    LastUsedAt,
    ExpiresAt,
}
