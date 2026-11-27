#![allow(clippy::unwrap_used, clippy::panic)]
use icatalog_engine::{dispatch, Context, Outcome, Params};

const PAINTINGS: &str = r#"{"any":[{"field":"format","value":"PTG, REB, CLIP"},{"field":"keyword","value":"painting"}]}"#;

fn fixture() -> Context {
    let mut context = Context::open_in_memory().unwrap();
    dispatch(&mut context, "init", &Params::new()).unwrap();
    context
        .catalog()
        .connection()
        .execute_batch(
            "INSERT INTO asset(id,uid,name,path,format) VALUES
        (1,'ptg','One.ptg','/One.ptg','PTG'),
        (2,'reb','Two.reb','/Two.reb','REB'),
        (3,'clip','Three.clip','/Three.clip','CLIP'),
        (4,'painting','Four.jpg','/Four.jpg','JPEG'),
        (5,'drawing','Five.png','/Five.png','PNG'),
        (6,'other','Six.jpg','/Six.jpg','JPEG');
        INSERT INTO annotation(asset_id,rating,label) VALUES (1,5,0),(2,3,0),(4,4,0),(5,5,0);
        INSERT INTO keyword(id,name) VALUES (1,'painting'),(2,'drawing');
        INSERT INTO asset_keyword(asset_id,keyword_id) VALUES (2,1),(2,2),(4,1),(5,2);",
        )
        .unwrap();
    context
}

fn run(context: &mut Context, pairs: &[(&str, &str)]) -> icatalog_engine::Result<Outcome> {
    let mut params = Params::new();
    for (key, value) in pairs {
        params.set(*key, *value);
    }
    dispatch(context, "collection", &params)
}

#[test]
fn paintings_or_and_nested_rules_preview_save_export_and_follow_edits() {
    let mut context = fixture();
    let before = context.catalog().connection().total_changes();
    let preview = run(
        &mut context,
        &[("action", "preview-smart"), ("rules-json", PAINTINGS)],
    )
    .unwrap()
    .render();
    assert!(preview.starts_with("4 matching items\n"), "{preview}");
    assert!(preview.contains(" OR "), "{preview}");
    assert_eq!(before, context.catalog().connection().total_changes());
    run(
        &mut context,
        &[
            ("action", "create-smart"),
            ("name", "Paintings"),
            ("rules-json", PAINTINGS),
        ],
    )
    .unwrap();
    assert_eq!(
        context.catalog().collection_assets(1).unwrap(),
        [1, 2, 3, 4]
    );
    let exported = run(
        &mut context,
        &[("action", "rules"), ("id", "1"), ("output", "json")],
    )
    .unwrap()
    .render();
    let states = context.catalog().collection_states().unwrap();
    let generation = context.catalog().generation().unwrap();
    run(
        &mut context,
        &[
            ("action", "set-rules"),
            ("id", "1"),
            ("name", "Paintings"),
            ("rules-json", &exported),
        ],
    )
    .unwrap();
    assert_eq!(context.catalog().collection_states().unwrap(), states);
    assert_eq!(context.catalog().generation().unwrap(), generation);

    let nested = format!(r#"{{"all":[{PAINTINGS},{{"field":"min-rating","value":4}}]}}"#);
    run(
        &mut context,
        &[
            ("action", "set-rules"),
            ("id", "1"),
            ("rules-json", &nested),
        ],
    )
    .unwrap();
    assert_eq!(context.catalog().collection_assets(1).unwrap(), [1, 4]);
    let summary = run(&mut context, &[("action", "rules"), ("id", "1")])
        .unwrap()
        .render();
    assert!(
        summary.contains(" OR ") && summary.contains(" AND ") && summary.contains('('),
        "{summary}"
    );
    let mut edit = Params::new();
    edit.set("id", "2").set("rating", "5");
    dispatch(&mut context, "edit", &edit).unwrap();
    assert_eq!(context.catalog().collection_assets(1).unwrap(), [1, 2, 4]);

    let and_rules = PAINTINGS.replacen("any", "all", 1);
    run(
        &mut context,
        &[
            ("action", "set-rules"),
            ("id", "1"),
            ("rules-json", &and_rules),
        ],
    )
    .unwrap();
    assert_eq!(context.catalog().collection_assets(1).unwrap(), [2]);
    let exact =
        r#"{"all":[{"field":"keyword","value":"painting"},{"field":"keyword","value":"drawing"}]}"#;
    run(
        &mut context,
        &[("action", "set-rules"), ("id", "1"), ("rules-json", exact)],
    )
    .unwrap();
    assert_eq!(context.catalog().collection_assets(1).unwrap(), [2]);
    let either = exact.replacen("all", "any", 1);
    run(
        &mut context,
        &[
            ("action", "set-rules"),
            ("id", "1"),
            ("rules-json", &either),
        ],
    )
    .unwrap();
    assert_eq!(context.catalog().collection_assets(1).unwrap(), [2, 4, 5]);
}

#[test]
fn invalid_grouped_inputs_and_failed_writes_preserve_name_rules_and_revisions() {
    let mut context = fixture();
    run(
        &mut context,
        &[
            ("action", "create-smart"),
            ("name", "Paintings"),
            ("rules-json", PAINTINGS),
        ],
    )
    .unwrap();
    let before = context.catalog().collection_states().unwrap();
    let generation = context.catalog().generation().unwrap();
    for json in [
        "",
        "null",
        "{}",
        r#"{"all":[]}"#,
        r#"{"any":[]}"#,
        r#"{"any":[{"field":"format","value":""}]}"#,
        r#"{"any":[{"field":"unknown","value":"painting"}]}"#,
        r#"{"any":[{"field":"min-rating","value":"4"}]}"#,
        r#"{"any":[{"field":"captured-from","value":"2026-02-30"}]}"#,
        r#"{"all":[],"any":[]}"#,
    ] {
        assert!(
            run(
                &mut context,
                &[
                    ("action", "set-rules"),
                    ("id", "1"),
                    ("name", "Changed"),
                    ("rules-json", json)
                ]
            )
            .is_err(),
            "{json}"
        );
    }
    for pairs in [
        vec![
            ("action", "preview-smart"),
            ("rules-json", PAINTINGS),
            ("format", "PTG"),
        ],
        vec![("action", "rules"), ("id", "1"), ("output", "xml")],
        vec![
            ("action", "set-rules"),
            ("id", "1"),
            ("rules-json", PAINTINGS),
            ("output", "json"),
        ],
        vec![
            ("action", "create"),
            ("name", "Manual"),
            ("rules-json", PAINTINGS),
        ],
    ] {
        assert!(run(&mut context, &pairs).is_err(), "{pairs:?}");
    }
    context.catalog().connection().execute_batch("CREATE TRIGGER fail_group BEFORE INSERT ON smart_rule_node BEGIN SELECT RAISE(ABORT,'synthetic grouped write failure'); END;").unwrap();
    let changed = PAINTINGS.replacen("any", "all", 1);
    assert!(run(
        &mut context,
        &[
            ("action", "set-rules"),
            ("id", "1"),
            ("name", "Changed"),
            ("rules-json", &changed)
        ]
    )
    .is_err());
    assert_eq!(context.catalog().collection_states().unwrap(), before);
    assert_eq!(context.catalog().generation().unwrap(), generation);
    assert!(context.catalog().connection().is_autocommit());
}

#[test]
fn hostile_grouped_parameters_return_errors_without_unwinding() {
    let mut context = fixture();
    let mut deep = r#"{"field":"format","value":"PTG"}"#.to_owned();
    for _ in 0..200 {
        deep = format!(r#"{{"any":[{deep}]}}"#);
    }
    for json in [
        deep,
        "[".repeat(100_000),
        format!(
            r#"{{"all":[{{"field":"keyword","value":"{}"}}]}}"#,
            "x".repeat(100_000)
        ),
    ] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(
                &mut context,
                &[("action", "preview-smart"), ("rules-json", &json)],
            )
        }));
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }
    assert!(context.catalog().all_collections().unwrap().is_empty());
}

#[test]
fn bounds_in_alternative_branches_are_independent() {
    let mut context = fixture();
    let json = r#"{"any":[{"field":"min-rating","value":4},{"field":"max-rating","value":2}]}"#;
    let preview = run(
        &mut context,
        &[("action", "preview-smart"), ("rules-json", json)],
    )
    .unwrap()
    .render();
    assert!(preview.starts_with("5 matching items\n"), "{preview}");
    let contradictory = json.replacen("any", "all", 1);
    let preview = run(
        &mut context,
        &[("action", "preview-smart"), ("rules-json", &contradictory)],
    )
    .unwrap()
    .render();
    assert!(preview.starts_with("0 matching items\n"), "{preview}");
    assert!(run(
        &mut context,
        &[
            ("action", "preview-smart"),
            ("min-rating", "4"),
            ("max-rating", "2")
        ]
    )
    .is_err());
}

#[test]
fn cli_conflict_details_show_differences_after_a_long_shared_rules_prefix() {
    use icatalog_engine::{
        SyncConflict, SyncEntityKind, SyncFieldConflict, SyncOutcome, SyncResolveResult, SyncRules,
        SyncValue,
    };
    let prefix = format!("Filename or path contains {:?} OR ", "a".repeat(1024));
    let local = format!("{prefix}Keyword is painting");
    let server = format!("{prefix}Keyword is drawing");
    let outcome = Outcome::Sync(SyncOutcome::Resolve(SyncResolveResult {
        conflicts: vec![SyncConflict {
            key: "collection:synthetic".into(),
            kind: SyncEntityKind::Collection,
            display_name: "Paintings".into(),
            fields: vec![SyncFieldConflict {
                field: "rules".into(),
                local: SyncValue::Rules(SyncRules {
                    summary: local.clone(),
                }),
                server: SyncValue::Rules(SyncRules {
                    summary: server.clone(),
                }),
                choice: None,
            }],
            shared: vec![],
        }],
        applied: false,
        recovery_backup: None,
        session_id: Some("synthetic-session".into()),
        total_conflicts: 1,
        page: 0,
        page_size: 50,
        all_choices_made: false,
        recovery_phase: None,
        recovery_pending: false,
        recovered: false,
    }));
    let rendered = outcome.render();
    assert!(rendered.contains(&local));
    assert!(rendered.contains(&server));
}
