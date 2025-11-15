//! SeaORM migration crate placeholder.
//! Define schema migrations and seeds here.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DatabaseConnection;

mod m20251101_000001_init;
mod m20251101_000002_seed_rule_templates;
mod m20251101_000003_seed_more_rule_templates;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20251101_000001_init::Migration),
            Box::new(m20251101_000002_seed_rule_templates::Migration),
            Box::new(m20251101_000003_seed_more_rule_templates::Migration),
        ]
    }
}

pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    Migrator::up(db, None).await
}

// Example structure for first migration (to be implemented later)
// pub mod m20250101_000001_init {
//     use super::*;
//     pub struct Migration;
//     #[async_trait::async_trait]
//     impl MigrationTrait for Migration {
//         async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
//             // manager.create_table(...).await
//             Ok(())
//         }
//         async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
//             Ok(())
//         }
//     }
// }
