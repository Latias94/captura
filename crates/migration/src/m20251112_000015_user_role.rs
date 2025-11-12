use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .add_column(
                        ColumnDef::new(User::Role)
                            .string_len(16)
                            .not_null()
                            .default("user"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(User::Table)
                    .drop_column(User::Role)
                    .to_owned(),
            )
            .await
    }
}

#[allow(dead_code)]
#[derive(DeriveIden)]
enum User {
    Table,
    Id,
    Username,
    PasswordHash,
    FeverKeyMd5,
    Role,
    CreatedAt,
}
