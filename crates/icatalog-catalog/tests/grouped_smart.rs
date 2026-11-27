#![allow(clippy::unwrap_used, clippy::panic)]
use icatalog_catalog::{AssetQuery, Catalog, SmartExpr, SmartPredicate, SmartRules};

fn fixture() -> Catalog {
    let c = Catalog::open_in_memory().unwrap();
    c.insert_catalogue("grouped-rules", "t").unwrap();
    c.connection().execute_batch("INSERT INTO asset(id,uid,name,path,format) VALUES
      (1,'a','One.ptg','/paint/One.ptg','PTG'),(2,'b','Two.reb','/paint/Two.reb','REB'),
      (3,'c','Three.clip','/paint/Three.clip','CLIP'),(4,'d','Four.jpg','/photo/Four.jpg','JPEG'),
      (5,'e','Five.png','/photo/Five.png','PNG'),(6,'f','Unknown','/unknown',NULL);
      INSERT INTO annotation(asset_id,rating,label) VALUES(1,5,2),(2,4,2),(4,2,1);
      INSERT INTO asset_meta(asset_id,captured_local,make,model,lens) VALUES
      (1,'2024-02-29T12:00:00','Example','Élite','Étoile'),(2,'2024-03-01T10:00:00','Other','Camera','50mm');
      INSERT INTO keyword(id,name) VALUES(1,'painting'),(2,'drawing'),(3,'paint, blue');
      INSERT INTO asset_keyword(asset_id,keyword_id) VALUES(2,1),(2,2),(4,1),(5,2),(5,3);").unwrap();
    c
}
fn leaf(p: SmartPredicate) -> SmartExpr {
    SmartExpr::Rule(p)
}
fn painting() -> SmartExpr {
    SmartExpr::Any(vec![
        leaf(SmartPredicate::Format("PTG, REB, CLIP".into())),
        leaf(SmartPredicate::Keyword("painting".into())),
    ])
}
fn saved(c: &Catalog, expr: SmartExpr) -> (i64, SmartRules) {
    let r = SmartRules::from_expression(expr).unwrap();
    let id = c.insert_collection("Smart").unwrap();
    c.set_smart_rules(id, &r).unwrap();
    (id, r)
}
fn assert_members(c: &Catalog, id: i64, r: &SmartRules, expected: &[i64]) {
    assert_eq!(c.smart_rules(id).unwrap(), Some(r.clone()));
    assert_eq!(c.collection_assets(id).unwrap(), expected);
    assert_eq!(c.count_smart_matches(r).unwrap(), expected.len() as i64);
    let counts = c.collection_asset_counts().unwrap();
    assert!(counts.contains(&(id, expected.len() as i64)));
    let assets = c
        .query_assets(&AssetQuery {
            collection_id: Some(id),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(assets.iter().map(|a| a.id).collect::<Vec<_>>(), expected);
    for asset in 1..=6 {
        assert_eq!(
            c.asset_collection_ids(asset).unwrap().contains(&id),
            expected.contains(&asset)
        );
    }
    let memberships = c.collection_memberships(&[1, 2, 3, 4, 5, 6, 1]).unwrap();
    assert_eq!(
        memberships
            .iter()
            .filter(|(_, collection)| *collection == id)
            .map(|(asset, _)| *asset)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn painting_or_and_nested_rules_share_every_membership_surface() {
    let c = fixture();
    let (or_id, or_rules) = saved(&c, painting());
    assert_members(&c, or_id, &or_rules, &[1, 2, 3, 4]);
    let (and_id, and_rules) = saved(
        &c,
        SmartExpr::All(vec![
            leaf(SmartPredicate::Format("PTG REB CLIP".into())),
            leaf(SmartPredicate::Keyword("painting".into())),
        ]),
    );
    assert_members(&c, and_id, &and_rules, &[2]);
    let (nested_id, nested) = saved(
        &c,
        SmartExpr::All(vec![painting(), leaf(SmartPredicate::MinRating(4))]),
    );
    assert_members(&c, nested_id, &nested, &[1, 2]);
    assert!(nested.summary().starts_with("(Format is any of"));
    assert!(nested
        .summary()
        .contains(" OR Keyword is \"painting\") AND Rating"));
    let manual = c.insert_collection("Manual").unwrap();
    c.add_to_collection(manual, 2).unwrap();
    c.add_to_collection(manual, 5).unwrap();
    let query = AssetQuery {
        collection_ids: Some(vec![or_id, nested_id, manual, or_id]),
        limit: Some(2),
        offset: Some(2),
        ..Default::default()
    };
    assert_eq!(
        c.query_assets(&query)
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );
    c.connection()
        .execute("UPDATE annotation SET rating=0 WHERE asset_id=1", [])
        .unwrap();
    assert_members(&c, nested_id, &nested, &[2]);
}

#[test]
fn every_field_works_in_any_and_all_with_missing_metadata_and_zero_defaults() {
    let c = fixture();
    for (predicate, expected) in [
        (SmartPredicate::Format(".jpe; .PTG".into()), vec![1, 4]),
        (SmartPredicate::CapturedFrom("2024-03-01".into()), vec![2]),
        (SmartPredicate::CapturedTo("2024-02-29".into()), vec![1]),
        (SmartPredicate::Camera("ÉLITE".into()), vec![1]),
        (SmartPredicate::Lens("éTOILE".into()), vec![1]),
        (SmartPredicate::MinRating(4), vec![1, 2]),
        (SmartPredicate::MaxRating(0), vec![3, 5, 6]),
        (SmartPredicate::Label(0), vec![3, 5, 6]),
        (SmartPredicate::Keyword("paint, blue".into()), vec![5]),
        (SmartPredicate::Text("PHOTO".into()), vec![4, 5]),
    ] {
        let (id, r) = saved(
            &c,
            SmartExpr::Any(vec![
                leaf(predicate.clone()),
                leaf(SmartPredicate::Text("not-present".into())),
            ]),
        );
        assert_members(&c, id, &r, &expected);
        let (id, r) = saved(
            &c,
            SmartExpr::All(vec![leaf(predicate), leaf(SmartPredicate::MinRating(0))]),
        );
        assert_members(&c, id, &r, &expected);
    }
}

#[test]
fn repeated_keywords_and_independent_range_branches_retain_meaning() {
    let c = fixture();
    let keywords = vec![
        leaf(SmartPredicate::Keyword("painting".into())),
        leaf(SmartPredicate::Keyword("drawing".into())),
    ];
    let (id, r) = saved(&c, SmartExpr::All(keywords.clone()));
    assert_members(&c, id, &r, &[2]);
    let (id, r) = saved(&c, SmartExpr::Any(keywords));
    assert_members(&c, id, &r, &[2, 4, 5]);
    let limits = vec![
        leaf(SmartPredicate::MinRating(4)),
        leaf(SmartPredicate::MaxRating(2)),
    ];
    let (id, r) = saved(&c, SmartExpr::Any(limits.clone()));
    assert_members(&c, id, &r, &[1, 2, 3, 4, 5, 6]);
    let (id, r) = saved(&c, SmartExpr::All(limits));
    assert!(r.expression.is_some());
    assert_members(&c, id, &r, &[]);
    let (id, r) = saved(
        &c,
        SmartExpr::Any(vec![
            leaf(SmartPredicate::CapturedFrom("2024-03-01".into())),
            leaf(SmartPredicate::CapturedTo("2024-02-29".into())),
        ]),
    );
    assert_members(&c, id, &r, &[1, 2]);
}

#[test]
fn json_roundtrip_is_strict_bounded_and_canonicalizes_legacy_all() {
    let legacy = SmartRules {
        format: Some("REB".into()),
        keyword: Some("painting".into()),
        ..Default::default()
    };
    let reordered = SmartRules::from_json(
        r#"{"all":[{"field":"keyword","value":"painting"},{"field":"format","value":"REB"}]}"#,
    )
    .unwrap();
    assert_eq!(legacy, reordered);
    let grouped = SmartRules::from_expression(painting()).unwrap();
    assert_eq!(
        SmartRules::from_json(&grouped.to_json().unwrap()).unwrap(),
        grouped
    );
    for json in [
        r#"{"all":[]}"#,
        r#"{"any":[]}"#,
        r#"{"field":"keyword","value":"painting"}"#,
        r#"{"all":[{"field":"rating","value":4}]}"#,
        r#"{"all":[{"field":"min-rating","value":1.5}]}"#,
        r#"{"all":[{"field":"label","value":8}]}"#,
        r#"{"all":[{"field":"captured-from","value":"2025-02-29"}]}"#,
        r#"{"all":[{"field":"keyword","value":""}]}"#,
        r#"{"all":[{"field":"keyword","value":"a\nb"}]}"#,
        r#"{"all":[{"field":"keyword","value":"a","extra":"b"}]}"#,
        r#"{"all":[{"field":"keyword","field":"text","value":"a"}]}"#,
        r#"{"all":[{"field":"keyword","value":"a","value":"b"}]}"#,
        r#"{"all":[{"field":"keyword","value":"a"}],"all":[{"field":"keyword","value":"b"}]}"#,
        r#"{"all":[{"field":"keyword","value":"a"}],"any":[{"field":"keyword","value":"b"}]}"#,
    ] {
        assert!(SmartRules::from_json(json).is_err(), "{json}");
    }
    let max = SmartExpr::Any(vec![leaf(SmartPredicate::Keyword("x".into())); 127]);
    assert!(SmartRules::from_expression(max).is_ok());
    assert!(SmartRules::from_expression(SmartExpr::Any(vec![
        leaf(SmartPredicate::Keyword(
            "x".into()
        ));
        128
    ]))
    .is_err());
    let mut deep = leaf(SmartPredicate::Text("x".into()));
    for _ in 0..8 {
        deep = SmartExpr::All(vec![deep]);
    }
    assert!(SmartRules::from_expression(deep.clone()).is_ok());
    assert!(SmartRules::from_expression(SmartExpr::All(vec![deep])).is_err());
    assert!(SmartRules::from_expression(SmartExpr::Any(vec![
        leaf(SmartPredicate::Keyword(
            "\"".repeat(1024)
        ));
        40
    ]))
    .is_err());
    assert!(SmartRules::from_json(&" ".repeat(65_537)).is_err());
    let empty = Catalog::open_in_memory().unwrap();
    assert!(empty
        .count_smart_matches(&SmartRules {
            expression: Some(SmartExpr::All(vec![])),
            ..Default::default()
        })
        .is_err());
}

#[test]
fn corruption_is_reported_instead_of_matching_every_asset() {
    for corruption in [
        "DELETE FROM smart_rule_node WHERE collection_id=1",
        "DELETE FROM smart_rule_node WHERE collection_id=1 AND ordinal>0",
        "UPDATE smart_rule_node SET parent=127 WHERE collection_id=1 AND ordinal=1",
        "UPDATE smart_rule_node SET parent=1 WHERE collection_id=1 AND ordinal=1",
        "UPDATE smart_rule_node SET kind=9 WHERE collection_id=1 AND ordinal=0",
        "UPDATE smart_rule_node SET int_value=99,field='min-rating',text_value=NULL WHERE collection_id=1 AND ordinal=1",
        "UPDATE smart_rule_node SET text_value='2025-02-29',field='captured-from' WHERE collection_id=1 AND ordinal=1",
        "UPDATE smart_rule_node SET text_value=replace(hex(zeroblob(1025)),'0','a') WHERE collection_id=1 AND ordinal=1",
    ] {
        let c=fixture();
        let (id,_)=saved(&c,painting());
        assert_eq!(id,1);
        c.connection().execute_batch("PRAGMA ignore_check_constraints=ON;").unwrap();
        c.connection().execute_batch(corruption).unwrap();
        assert!(c.smart_rules(id).is_err(),"{corruption}");
        assert!(c.collection_assets(id).is_err(),"{corruption}");
        assert!(c.collection_asset_counts().is_err(),"{corruption}");
    }
}

#[test]
fn saves_are_noops_or_atomic_even_without_an_outer_transaction() {
    let c = fixture();
    let (id, r) = saved(&c, painting());
    let before = c.collection_states().unwrap();
    let generation = c.generation().unwrap();
    c.set_smart_rules(id, &r).unwrap();
    assert_eq!(c.collection_states().unwrap(), before);
    assert_eq!(c.generation().unwrap(), generation);
    c.connection().execute_batch("CREATE TRIGGER fail_rule BEFORE INSERT ON smart_rule_node WHEN NEW.ordinal=2 BEGIN SELECT RAISE(ABORT,'synthetic disk failure'); END;").unwrap();
    let changed = SmartRules::from_expression(SmartExpr::All(vec![
        painting(),
        leaf(SmartPredicate::MinRating(4)),
    ]))
    .unwrap();
    assert!(c.set_smart_rules(id, &changed).is_err());
    assert_eq!(c.collection_states().unwrap(), before);
    assert_eq!(c.generation().unwrap(), generation);
    assert_members(&c, id, &r, &[1, 2, 3, 4]);
}

fn v8(path: &std::path::Path) -> (i64, i64, String) {
    let c = rusqlite::Connection::open(path).unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_utc TEXT NOT NULL);").unwrap();
    for (i, sql) in [
        include_str!("../src/v1.sql"),
        include_str!("../src/v2.sql"),
        include_str!("../src/v3.sql"),
        include_str!("../src/v4.sql"),
        include_str!("../src/v5.sql"),
        include_str!("../src/v6.sql"),
        include_str!("../src/v7.sql"),
        include_str!("../src/v8.sql"),
    ]
    .into_iter()
    .enumerate()
    {
        c.execute_batch(sql).unwrap();
        c.execute("INSERT INTO schema_migrations VALUES(?1,'t')", [i + 1])
            .unwrap();
    }
    c.execute_batch("INSERT INTO catalogue VALUES('v8-grouped',0,'t',8);INSERT INTO asset(id,uid,name,path,format) VALUES(1,'a','a','/a','REB');INSERT INTO keyword(id,name) VALUES(1,'painting');INSERT INTO asset_keyword VALUES(1,1);INSERT INTO collection(id,name) VALUES(1,'Manual'),(2,'Smart');INSERT INTO collection_asset VALUES(1,1);INSERT INTO smart_collection(collection_id,format,keyword,min_rating,max_rating) VALUES(2,'REB','painting',0,5);INSERT INTO replica_base VALUES(1,0,x'010203');").unwrap();
    c.query_row("SELECT generation,(SELECT revision FROM sync_revision WHERE kind='collection' AND entity_id=2),(SELECT uid FROM collection WHERE id=2) FROM catalogue",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}

#[test]
fn v9_migration_preserves_legacy_rules_identity_revisions_baseline_and_reopen() {
    let path = std::env::temp_dir().join(format!("icat-v9-preserve-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let (generation, revision, uid) = v8(&path);
    let c = Catalog::open(&path).unwrap();
    assert_eq!(c.schema_version().unwrap(), 9);
    assert_eq!(c.generation().unwrap(), Some(generation + 1));
    let r = c.smart_rules(2).unwrap().unwrap();
    assert!(r.expression.is_none());
    assert_eq!(r.format.as_deref(), Some("REB"));
    assert_eq!(r.keyword.as_deref(), Some("painting"));
    assert_eq!((r.min_rating, r.max_rating), (Some(0), Some(5)));
    assert_eq!(c.collection_assets(1).unwrap(), vec![1]);
    assert_eq!(c.collection_assets(2).unwrap(), vec![1]);
    let state = c
        .collection_states()
        .unwrap()
        .into_iter()
        .find(|s| s.uid == uid)
        .unwrap();
    assert_eq!(state.revision, revision);
    assert_eq!(c.replica_base().unwrap(), Some((0, vec![1, 2, 3])));
    assert!(c.add_to_collection(2, 1).is_err());
    assert!(c
        .connection()
        .execute("UPDATE collection_asset SET collection_id=2", [])
        .is_err());
    c.set_smart_rules(2, &r).unwrap();
    assert_eq!(c.generation().unwrap(), Some(generation + 1));
    drop(c);
    let c = Catalog::open(&path).unwrap();
    assert_eq!(c.generation().unwrap(), Some(generation + 1));
    assert_eq!(c.smart_rules(2).unwrap(), Some(r));
    drop(c);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn failed_v9_migration_keeps_v8_and_can_retry() {
    let path = std::env::temp_dir().join(format!("icat-v9-rollback-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    v8(&path);
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute("UPDATE catalogue SET generation=?1", [i64::MAX])
            .unwrap();
    }
    assert!(Catalog::open(&path).is_err());
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(
            c.query_row("SELECT max(version) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            8
        );
        assert_eq!(
            c.query_row(
                "SELECT format FROM smart_collection WHERE collection_id=2",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "REB"
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='smart_rule_node'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        c.execute("UPDATE catalogue SET generation=0", []).unwrap();
    }
    let c = Catalog::open(&path).unwrap();
    assert_eq!(c.schema_version().unwrap(), 9);
    assert_eq!(c.collection_assets(2).unwrap(), vec![1]);
    drop(c);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn nested_rules_support_thousands_of_assets_and_32000_selected_collection_ids() {
    let c = Catalog::open_in_memory().unwrap();
    c.insert_catalogue("group-scale", "t").unwrap();
    c.connection().execute_batch("WITH RECURSIVE numbers(n) AS(VALUES(1) UNION ALL SELECT n+1 FROM numbers WHERE n<2000) INSERT INTO asset(id,uid,name,path,format) SELECT n,'asset:'||n,'file:'||n,'/'||n,CASE WHEN n%2=0 THEN 'PTG' ELSE 'PNG' END FROM numbers;").unwrap();
    let expr = SmartExpr::All(vec![
        painting(),
        SmartExpr::Any(vec![
            leaf(SmartPredicate::MaxRating(0)),
            leaf(SmartPredicate::Keyword("not-present".into())),
        ]),
    ]);
    let (id, r) = saved(&c, expr);
    assert_eq!(c.count_smart_matches(&r).unwrap(), 1000);
    assert_eq!(c.collection_asset_counts().unwrap(), vec![(id, 1000)]);
    let assets = c
        .query_assets(&AssetQuery {
            collection_ids: Some((1..=32_000).collect()),
            limit: Some(10),
            offset: Some(20),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        assets.iter().map(|a| a.id).collect::<Vec<_>>(),
        (42..=60).step_by(2).collect::<Vec<_>>()
    );
}

#[test]
fn old_read_only_catalogues_use_connection_local_nodes_without_changing_file_bytes() {
    for version in 6..=8 {
        let path = std::env::temp_dir().join(format!(
            "icat-v9-readonly-v{version}-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,applied_utc TEXT NOT NULL);").unwrap();
            for (i, sql) in [
                include_str!("../src/v1.sql"),
                include_str!("../src/v2.sql"),
                include_str!("../src/v3.sql"),
                include_str!("../src/v4.sql"),
                include_str!("../src/v5.sql"),
                include_str!("../src/v6.sql"),
                include_str!("../src/v7.sql"),
                include_str!("../src/v8.sql"),
            ]
            .into_iter()
            .take(version)
            .enumerate()
            {
                c.execute_batch(sql).unwrap();
                c.execute("INSERT INTO schema_migrations VALUES(?1,'t')", [i + 1])
                    .unwrap();
            }
            c.execute(
                "INSERT INTO catalogue VALUES('readonly',0,'t',?1)",
                [version],
            )
            .unwrap();
            c.execute_batch("INSERT INTO asset(id,uid,name,path,format) VALUES(1,'a','a','/a','REB');INSERT INTO collection(id,name) VALUES(1,'Smart');INSERT INTO smart_collection(collection_id,format,min_rating) VALUES(1,'REB',0);").unwrap();
            if version == 8 {
                c.execute("UPDATE smart_collection SET max_rating=5", [])
                    .unwrap();
            }
        }
        let before = std::fs::read(&path).unwrap();
        let c = Catalog::open_read_only(&path).unwrap();
        let rules = c.smart_rules(1).unwrap().unwrap();
        assert_eq!(rules.format.as_deref(), Some("REB"));
        assert_eq!(rules.max_rating, if version == 8 { Some(5) } else { None });
        assert_eq!(c.collection_assets(1).unwrap(), vec![1]);
        assert_eq!(c.collection_asset_counts().unwrap(), vec![(1, 1)]);
        if version >= 7 {
            assert_eq!(c.collection_states().unwrap().len(), 1);
        }
        assert_eq!(c.count_smart_matches(&rules).unwrap(), 1);
        assert_eq!(c.schema_version().unwrap(), version as i64);
        drop(c);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        // Adopting a reserved existing connection also skips migration.
        let c =
            Catalog::from_existing_connection(rusqlite::Connection::open(&path).unwrap()).unwrap();
        assert_eq!(c.smart_rules(1).unwrap(), Some(rules));
        drop(c);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn damaged_v9_nodes_do_not_receive_legacy_fallback() {
    let path = std::env::temp_dir().join(format!(
        "icat-v9-missing-nodes-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let c = Catalog::open(&path).unwrap();
        c.insert_catalogue("missing-nodes", "t").unwrap();
        saved(&c, painting());
        c.connection()
            .execute_batch("DROP TABLE smart_rule_node")
            .unwrap();
    }
    let c = Catalog::open_read_only(&path).unwrap();
    assert!(c.smart_rules(1).is_err());
    assert_eq!(
        c.connection()
            .query_row(
                "SELECT count(*) FROM sqlite_temp_schema WHERE name='smart_rule_node'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    drop(c);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn escaped_json_size_limit_agrees_for_drafts_and_stored_membership() {
    let c = fixture();
    let text = "\"".repeat(1024);
    let expression = SmartExpr::Any(vec![leaf(SmartPredicate::Text(text.clone())); 31]);
    let (id, rules) = saved(&c, expression);
    assert!(rules.to_json().unwrap().len() <= 65_536);
    assert_eq!(c.count_smart_matches(&rules).unwrap(), 0);
    assert!(SmartRules::from_expression(SmartExpr::Any(vec![
        leaf(SmartPredicate::Text(
            text.clone()
        ));
        32
    ]))
    .is_err());
    // Every row is individually legal, but the complete encoded expression
    // exceeds the transport limit. Saved membership must reject it too.
    c.connection().execute(
        "INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) VALUES(?1,32,0,2,'text',?2)",
        rusqlite::params![id, text],
    ).unwrap();
    assert!(c.smart_rules(id).is_err());
    assert!(c.collection_assets(id).is_err());
    assert!(c.collection_asset_counts().is_err());
}
