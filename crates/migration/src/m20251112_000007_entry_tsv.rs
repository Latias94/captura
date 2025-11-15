use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        if db.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Ok(());
        }
        // Note: use triggers to keep tsv in sync and avoid issues with generated columns on older Postgres versions
        let stmts = [
            // Column and index
            "ALTER TABLE entry ADD COLUMN IF NOT EXISTS tsv tsvector;",
            // Pre-fill, limiting each segment to 500k characters and setting weights
            r#"
            UPDATE entry SET tsv =
                setweight(to_tsvector('simple', left(coalesce(title,''), 500000)), 'A') ||
                setweight(to_tsvector('simple', left(coalesce(summary,''), 500000)), 'B') ||
                setweight(to_tsvector('simple', left(coalesce(content_html,''), 500000)), 'C');
            "#,
            "CREATE INDEX IF NOT EXISTS idx_entry_tsv ON entry USING GIN (tsv);",
            // Trigger function
            r#"
            CREATE OR REPLACE FUNCTION entry_tsv_update() RETURNS trigger AS $$
            BEGIN
                NEW.tsv :=
                    setweight(to_tsvector('simple', left(coalesce(NEW.title,''), 500000)), 'A') ||
                    setweight(to_tsvector('simple', left(coalesce(NEW.summary,''), 500000)), 'B') ||
                    setweight(to_tsvector('simple', left(coalesce(NEW.content_html,''), 500000)), 'C');
                RETURN NEW;
            END
            $$ LANGUAGE plpgsql;
            "#,
            // Trigger
            "DROP TRIGGER IF EXISTS entry_tsv_update ON entry;",
            "CREATE TRIGGER entry_tsv_update BEFORE INSERT OR UPDATE OF title, summary, content_html ON entry FOR EACH ROW EXECUTE FUNCTION entry_tsv_update();",
        ];
        for sql in stmts {
            db.execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        if db.get_database_backend() != sea_orm::DatabaseBackend::Postgres {
            return Ok(());
        }
        let stmts = [
            "DROP TRIGGER IF EXISTS entry_tsv_update ON entry;",
            "DROP FUNCTION IF EXISTS entry_tsv_update();",
            "DROP INDEX IF EXISTS idx_entry_tsv;",
            "ALTER TABLE entry DROP COLUMN IF EXISTS tsv;",
        ];
        for sql in stmts {
            db.execute_unprepared(sql).await?;
        }
        Ok(())
    }
}
