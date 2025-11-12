use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserPref::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserPref::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UserPref::UserId).big_integer().not_null())
                    .col(ColumnDef::new(UserPref::Key).string_len(64).not_null())
                    .col(ColumnDef::new(UserPref::ValueJson).json_binary())
                    .col(
                        ColumnDef::new(UserPref::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(UserPref::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_userpref_user")
                            .from(UserPref::Table, UserPref::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_userpref_user_key")
                    .table(UserPref::Table)
                    .col(UserPref::UserId)
                    .col(UserPref::Key)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserPref::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum UserPref {
    Table,
    Id,
    UserId,
    Key,
    ValueJson,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}
