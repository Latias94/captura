//! Utility for syncing YAML rules under `rules/` directory into the database.
//!
//! Design goals:
//! - Treat files as the single source of truth for official/community rules; DB is just a runtime mirror.
//! - Provide a minimal viable implementation for now: upsert by `rule_id` without tracking user modifications.
//! - Allow future extensions on the rule table (origin/source_hash/user_modified, etc.).

use std::fs;
use std::path::{Path, PathBuf};

use captura_common::{Error, Result};
use captura_rules::v1::parse_rule_v1;
use captura_storage::entity::rule;
use chrono::{FixedOffset, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use tracing::{info, warn};

/// Sync result statistics.
#[derive(Debug, Clone)]
pub struct RulesSyncReport {
    pub scanned_files: usize,
    pub created: usize,
    pub updated: usize,
    pub failed: usize,
}

fn collect_yaml_files(root: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_yaml_files(&path, out)?;
        } else if let Some(ext) = path.extension() {
            if ext == "yaml" || ext == "yml" {
                out.push(path);
            }
        }
    }
    Ok(())
}

fn infer_namespace(rule_id: &str) -> Option<String> {
    rule_id.rsplit_once('.').map(|(ns, _)| ns.to_string())
}

/// Sync rule files from the given root directory into the database.
///
/// Current strategy (v1):
/// - Scan all `.yaml/.yml` files under `root`;
/// - Parse each file into `RuleSpecV1`;
/// - Use `rule_id` as logical primary key:
///   - not exists → INSERT;
///   - exists → UPDATE (overwrite yaml/description/examples directly).
/// - Any parse or IO errors are counted as `failed` but do not abort the whole sync.
pub async fn sync_rules_from_fs(db: &DatabaseConnection, root: &Path) -> Result<RulesSyncReport> {
    let mut files = Vec::new();
    collect_yaml_files(root, &mut files).map_err(|e| Error::Config(e.to_string()))?;
    files.sort();

    let mut report = RulesSyncReport {
        scanned_files: files.len(),
        created: 0,
        updated: 0,
        failed: 0,
    };

    if files.is_empty() {
        info!("rules_sync: no yaml files found under {:?}", root);
        return Ok(report);
    }

    let now = Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap());

    for path in files {
        let file_path = path.display().to_string();
        let yaml = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    file = %file_path,
                    error = %e,
                    "rules_sync: failed to read yaml file"
                );
                report.failed += 1;
                continue;
            }
        };
        let spec = match parse_rule_v1(&yaml) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    file = %file_path,
                    error = %e,
                    "rules_sync: invalid rule yaml"
                );
                report.failed += 1;
                continue;
            }
        };

        let examples = match serde_json::to_value(&spec.examples) {
            Ok(v) => Some(v),
            Err(e) => {
                warn!(
                    file = %file_path,
                    error = %e,
                    "rules_sync: failed to encode examples_json"
                );
                report.failed += 1;
                continue;
            }
        };

        let existing = rule::Entity::find()
            .filter(rule::Column::RuleId.eq(spec.id.clone()))
            .one(db)
            .await
            .map_err(|e| Error::Storage(e.to_string()))?;

        if let Some(rec) = existing {
            let mut am: rule::ActiveModel = rec.into();
            am.namespace = Set(infer_namespace(&spec.id));
            am.description = Set(spec.description.clone());
            am.yaml = Set(yaml.clone());
            am.examples_json = Set(examples);
            am.updated_at = Set(now);
            am.update(db)
                .await
                .map_err(|e| Error::Storage(e.to_string()))?;
            report.updated += 1;
        } else {
            let am = rule::ActiveModel {
                rule_id: Set(spec.id.clone()),
                version: Set(None),
                namespace: Set(infer_namespace(&spec.id)),
                description: Set(spec.description.clone()),
                yaml: Set(yaml.clone()),
                examples_json: Set(examples),
                verified_at: Set(Some(now)),
                maintainer: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };
            let _ = am
                .insert(db)
                .await
                .map_err(|e| Error::Storage(e.to_string()))?;
            report.created += 1;
        }
    }

    info!(
        root = ?root,
        created = report.created,
        updated = report.updated,
        failed = report.failed,
        "rules_sync: completed"
    );

    Ok(report)
}
