//! SeaORM migration crate placeholder.
//! Define schema migrations and seeds here.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DatabaseConnection;

mod m20251101_000001_init;
mod m20251101_000002_user_fever;
mod m20251111_000003_favicon;
mod m20251111_000004_token_expiry;
mod m20251111_000005_enclosure_progress;
mod m20251111_000006_token_plain;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20251101_000001_init::Migration),
            Box::new(m20251101_000002_user_fever::Migration),
            Box::new(m20251111_000003_favicon::Migration),
            Box::new(m20251111_000004_token_expiry::Migration),
            Box::new(m20251111_000006_token_plain::Migration),
            Box::new(m20251111_000005_enclosure_progress::Migration),
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
