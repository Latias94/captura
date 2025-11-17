use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SmartView::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SmartView::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SmartView::UserId).big_integer().not_null())
                    .col(
                        ColumnDef::new(SmartView::Name)
                            .string_len(190)
                            .not_null(),
                    )
                    .col(ColumnDef::new(SmartView::View).string().not_null())
                    .col(ColumnDef::new(SmartView::FiltersJson).json_binary())
                    .col(ColumnDef::new(SmartView::SortBy).string_len(32))
                    .col(ColumnDef::new(SmartView::SortOrder).string_len(8))
                    .col(
                        ColumnDef::new(SmartView::Pinned)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(SmartView::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SmartView::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_smart_view_user")
                            .from(SmartView::Table, SmartView::UserId)
                            .to(User::Table, User::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SmartView::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum SmartView {
    Table,
    Id,
    UserId,
    Name,
    View,
    FiltersJson,
    SortBy,
    SortOrder,
    Pinned,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum User {
    Table,
    Id,
}

