use captura_service::search::parse_query;

#[test]
fn parse_query_extracts_fields_and_tags() {
    let q = r#"hello world title:"rust timeline" author:bob url:example.com #tag1 #"tag 2""#;
    let parsed = parse_query(q);

    // General text should keep the free words and strip field/tag parts.
    let general = parsed.general.expect("general part missing");
    assert!(general.contains("hello"));
    assert!(general.contains("world"));
    assert!(!general.contains("title:"));
    assert!(!general.contains("#tag1"));

    // Field filters.
    assert_eq!(parsed.title, vec!["rust timeline".to_string()]);
    assert_eq!(parsed.author, vec!["bob".to_string()]);
    assert_eq!(parsed.url, vec!["example.com".to_string()]);

    // Tags should be normalized and preserve quoted spaces.
    assert_eq!(parsed.tags, vec!["tag1".to_string(), "tag 2".to_string()]);
}
