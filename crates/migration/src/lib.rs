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
mod m20251112_000007_entry_tsv;
mod m20251112_000008_webhook;
mod m20251112_000009_integration;
mod m20251112_000010_job_payload_integration;
mod m20251112_000011_feed_integrations;
mod m20251112_000012_feed_rule_params;
mod m20251112_000013_seed_rule_templates;
mod m20251112_000014_seed_more_rule_templates;
mod m20251112_000015_user_role;
mod m20251112_000016_user_prefs;

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
            Box::new(m20251112_000007_entry_tsv::Migration),
            Box::new(m20251112_000008_webhook::Migration),
            Box::new(m20251112_000009_integration::Migration),
            Box::new(m20251112_000010_job_payload_integration::Migration),
            Box::new(m20251112_000011_feed_integrations::Migration),
            Box::new(m20251112_000012_feed_rule_params::Migration),
            Box::new(m20251112_000013_seed_rule_templates::Migration),
            Box::new(m20251112_000014_seed_more_rule_templates::Migration),
            Box::new(m20251112_000015_user_role::Migration),
            Box::new(m20251112_000016_user_prefs::Migration),
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
