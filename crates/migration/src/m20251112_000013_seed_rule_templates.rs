use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // Three rule templates: GitHub Trending, Hacker News, Lobsters
        let tpl_github = r#"id: captura.route.github.trending
description: GitHub Trending repositories
fetch:
  user_agent: captura/0.1
list:
  url: "https://github.com/trending?since={since}"
  item: "article.Box-row"
  link: "h2 a@href"
  title: "h2 a"
content:
  use: readability
"#;
        let tpl_hn = r#"id: captura.route.hn.front
description: Hacker News Front Page
fetch:
  user_agent: captura/0.1
list:
  url: "https://news.ycombinator.com/"
  item: "tr.athing"
  link: "span.titleline a@href"
  title: "span.titleline a"
content:
  use: readability
"#;
        let tpl_lobsters = r#"id: captura.route.lobsters.front
description: Lobsters Front Page
fetch:
  user_agent: captura/0.1
list:
  url: "https://lobste.rs/"
  item: "li.story"
  link: "h2 a@href"
  title: "h2 a"
content:
  use: readability
"#;
        let stmts = [
            (
                "captura.route.github.trending",
                "captura.route",
                tpl_github,
                "[\"since=daily\",\"since=weekly\",\"since=monthly\"]",
            ),
            ("captura.route.hn.front", "captura.route", tpl_hn, "[]"),
            (
                "captura.route.lobsters.front",
                "captura.route",
                tpl_lobsters,
                "[]",
            ),
        ];
        for (rid, ns, yaml, examples) in stmts {
            let yaml_escaped = yaml.replace("'", "''");
            let sql = format!(
                "INSERT INTO rule (rule_id, version, namespace, description, yaml, examples_json, verified_at, maintainer, created_at, updated_at) \
                 VALUES ('{}','0.1','{}',NULL,'{}','{}', CURRENT_TIMESTAMP, 'captura', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
                 ON CONFLICT(rule_id) DO NOTHING",
                rid, ns, yaml_escaped, examples
            );
            db.execute_unprepared(sql.as_str()).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        let ids = [
            "captura.route.github.trending",
            "captura.route.hn.front",
            "captura.route.lobsters.front",
        ];
        for rid in ids {
            let sql = format!("DELETE FROM rule WHERE rule_id='{}'", rid);
            db.execute_unprepared(sql.as_str()).await?;
        }
        Ok(())
    }
}
