#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_icatalog"))
        .args(args)
        .output()
        .unwrap()
}
fn success(args: &[&str]) -> String {
    let output = cli(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn grouped_rules_round_trip_through_cli_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("grouped.icatalog");
    let path = path.to_str().unwrap();
    success(&["init", path]);
    let rules = r#"{"all":[{"any":[{"field":"format","value":"PTG, REB, CLIP"},{"field":"keyword","value":"painting"}]},{"field":"min-rating","value":4}]}"#;
    let preview = success(&["collection", path, "preview-smart", "--rules-json", rules]);
    assert!(
        preview.contains("0 matching items")
            && preview.contains(" OR ")
            && preview.contains(" AND "),
        "{preview}"
    );
    success(&[
        "collection",
        path,
        "create-smart",
        "--name",
        "Paintings",
        "--rules-json",
        rules,
    ]);
    let exported = success(&["collection", path, "rules", "--id", "1", "--output", "json"]);
    assert!(exported.contains("\"any\""));
    success(&[
        "collection",
        path,
        "set-rules",
        "--id",
        "1",
        "--name",
        "Favourite paintings",
        "--rules-json",
        exported.trim(),
    ]);
    let reopened = success(&["collection", path, "rules", "--id", "1", "--output", "json"]);
    assert_eq!(reopened, exported);
    let before = std::fs::read(path).unwrap();
    for args in [
        vec![
            "collection",
            path,
            "set-rules",
            "--id",
            "1",
            "--name",
            "Bad",
            "--rules-json",
            r#"{"any":[]}"#,
        ],
        vec![
            "collection",
            path,
            "preview-smart",
            "--rules-json",
            rules,
            "--format",
            "PTG",
        ],
        vec!["collection", path, "rules", "--id", "1", "--output", "yaml"],
    ] {
        let output = cli(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    let help = success(&["--help"]);
    assert!(help.contains("--rules-json") && help.contains("--output"));
}
