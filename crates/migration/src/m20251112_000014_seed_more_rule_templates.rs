use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let tpl_zhihu = r#"id: captura.route.zhihu.hotlist
description: Zhihu Hot List
fetch:
  user_agent: captura/0.1
list:
  url: "https://www.zhihu.com/hot"
  item: "div.HotItem"
  link: "a.HotItem-title@href"
  title: "a.HotItem-title"
content:
  use: readability
"#;

        let tpl_reuters = r#"id: captura.route.reuters.top
description: Reuters Top News
fetch:
  user_agent: captura/0.1
list:
  url: "https://www.reuters.com/world/"
  item: "article.story-card, article.story"
  link: "a@href"
  title: "h3, h2"
content:
  use: readability
"#;

        let tpl_medium_tag = r#"id: captura.route.medium.tag
description: Medium by Tag
fetch:
  user_agent: captura/0.1
list:
  url: "https://medium.com/tag/{tag}/latest"
  item: "div.postArticle"
  link: "a.ds-link, a.link--primary@href"
  title: "h3, h2"
content:
  use: readability
"#;

        let stmts = [
            (
                "captura.route.zhihu.hotlist",
                "captura.route",
                tpl_zhihu,
                "[]",
            ),
            (
                "captura.route.reuters.top",
                "captura.route",
                tpl_reuters,
                "[]",
            ),
            (
                "captura.route.medium.tag",
                "captura.route",
                tpl_medium_tag,
                "[\"tag=rust\",\"tag=ai\"]",
            ),
        ];

        for (rid, ns, yaml, examples) in stmts {
            let yaml_escaped = yaml.replace("'", "''");
            let sql = format!(
                "INSERT INTO rule (rule_id, version, namespace, description, yaml, examples_json, verified_at, maintainer, created_at, updated_at) \
                 VALUES ('{}','0.1','{}',NULL,'{}',{}, CURRENT_TIMESTAMP, 'captura', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) \
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
            "captura.route.zhihu.hotlist",
            "captura.route.reuters.top",
            "captura.route.medium.tag",
        ];
        for rid in ids {
            let sql = format!("DELETE FROM rule WHERE rule_id='{}'", rid);
            db.execute_unprepared(sql.as_str()).await?;
        }
        Ok(())
    }
}
