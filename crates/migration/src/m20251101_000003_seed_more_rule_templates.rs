use sea_orm_migration::prelude::*;

/// This migration previously seeded additional rule templates using an
/// older, pre-v1 DSL format. It is now intentionally a no-op because
/// those routes are implemented as Hub handlers instead.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
