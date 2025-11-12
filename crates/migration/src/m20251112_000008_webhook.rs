use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Webhook::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Webhook::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Webhook::UserId).big_integer().not_null())
                    .col(ColumnDef::new(Webhook::Url).string().not_null())
                    .col(ColumnDef::new(Webhook::Secret).string().not_null())
                    .col(ColumnDef::new(Webhook::Events).string())
                    .col(
                        ColumnDef::new(Webhook::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Webhook::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Webhook::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_webhook_user")
                            .from(Webhook::Table, Webhook::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Webhook::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Webhook {
    Table,
    Id,
    UserId,
    Url,
    Secret,
    Events,
    Enabled,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}
