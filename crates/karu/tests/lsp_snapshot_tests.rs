//! Snapshot-based tests for LSP functions.
//!
//! Each `.karu` fixture in `tests/lsp_fixtures/` can have companion files:
//!   - `.diagnostics.json`  - expected `parse_diagnostics()` output
//!   - `.symbols.json`      - expected `document_symbols()` names/kinds
//!   - `.formatted.karu`    - expected output from `format_source()`
//!
//! Run with `UPDATE_SNAPSHOTS=1` to regenerate all snapshot files.

use karu::format::format_source;
use karu::lsp::{document_symbols, find_rule_locations, parse_diagnostics, semantic_tokens};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// ── Snapshot types ──────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct DiagnosticSnapshot {
    line: u32,
    character: u32,
    severity: String,
    message_contains: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct SymbolSnapshot {
    name: String,
    kind: String,
}

// ── Helpers ─────────────────────────────────────────────────────────

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/lsp_fixtures")
}

fn load_fixture(name: &str) -> String {
    fs::read_to_string(fixtures_dir().join(name))
        .unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", name, e))
}

fn should_update() -> bool {
    std::env::var("UPDATE_SNAPSHOTS").is_ok()
}

fn severity_str(sev: tower_lsp::lsp_types::DiagnosticSeverity) -> String {
    match sev {
        tower_lsp::lsp_types::DiagnosticSeverity::ERROR => "error".to_string(),
        tower_lsp::lsp_types::DiagnosticSeverity::WARNING => "warning".to_string(),
        tower_lsp::lsp_types::DiagnosticSeverity::INFORMATION => "info".to_string(),
        tower_lsp::lsp_types::DiagnosticSeverity::HINT => "hint".to_string(),
        _ => "unknown".to_string(),
    }
}

fn symbol_kind_str(kind: tower_lsp::lsp_types::SymbolKind) -> String {
    match kind {
        tower_lsp::lsp_types::SymbolKind::FUNCTION => "Function".to_string(),
        tower_lsp::lsp_types::SymbolKind::EVENT => "Event".to_string(),
        tower_lsp::lsp_types::SymbolKind::VARIABLE => "Variable".to_string(),
        tower_lsp::lsp_types::SymbolKind::NAMESPACE => "Namespace".to_string(),
        _ => format!("{:?}", kind),
    }
}

/// Find all .karu files in the fixtures directory that have a companion snapshot.
fn fixture_pairs(suffix: &str) -> Vec<(String, PathBuf)> {
    let dir = fixtures_dir();
    let mut pairs = Vec::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "karu")
            && !path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(".formatted")
        {
            let snap_path = path.with_extension(format!("{}.json", suffix));
            // Include if snapshot exists or we're in update mode
            if snap_path.exists() || should_update() {
                let name = path.file_name().unwrap().to_str().unwrap().to_string();
                pairs.push((name, snap_path));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

// ── Diagnostic snapshot tests ───────────────────────────────────────

#[test]
fn test_diagnostics_snapshots() {
    let pairs = fixture_pairs("diagnostics");
    assert!(!pairs.is_empty(), "No diagnostic fixtures found");

    for (karu_name, snap_path) in &pairs {
        let source = load_fixture(karu_name);
        let diagnostics = parse_diagnostics(&source);

        let actual: Vec<DiagnosticSnapshot> = diagnostics
            .iter()
            .map(|d| DiagnosticSnapshot {
                line: d.range.start.line,
                character: d.range.start.character,
                severity: d
                    .severity
                    .map(severity_str)
                    .unwrap_or_else(|| "unknown".to_string()),
                message_contains: d.message.clone(),
            })
            .collect();

        if should_update() {
            let json = serde_json::to_string_pretty(&actual).unwrap();
            fs::write(snap_path, format!("{}\n", json)).unwrap();
            println!("Updated snapshot: {}", snap_path.display());
            continue;
        }

        let expected_json = fs::read_to_string(snap_path)
            .unwrap_or_else(|e| panic!("Missing snapshot {}: {}", snap_path.display(), e));
        let expected: Vec<DiagnosticSnapshot> = serde_json::from_str(&expected_json)
            .unwrap_or_else(|e| panic!("Bad JSON in {}: {}", snap_path.display(), e));

        assert_eq!(
            actual.len(),
            expected.len(),
            "Diagnostic count mismatch for {}: got {} expected {}\nActual: {:#?}",
            karu_name,
            actual.len(),
            expected.len(),
            actual,
        );

        for (i, (act, exp)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                act.line, exp.line,
                "{} diagnostic #{}: line mismatch",
                karu_name, i
            );
            assert_eq!(
                act.character, exp.character,
                "{} diagnostic #{}: character mismatch",
                karu_name, i
            );
            assert_eq!(
                act.severity, exp.severity,
                "{} diagnostic #{}: severity mismatch",
                karu_name, i
            );
            assert!(
                act.message_contains.contains(&exp.message_contains),
                "{} diagnostic #{}: message '{}' does not contain '{}'",
                karu_name,
                i,
                act.message_contains,
                exp.message_contains,
            );
        }
    }
}

// ── Symbol snapshot tests ───────────────────────────────────────────

#[test]
fn test_symbols_snapshots() {
    let pairs = fixture_pairs("symbols");
    assert!(!pairs.is_empty(), "No symbol fixtures found");

    for (karu_name, snap_path) in &pairs {
        let source = load_fixture(karu_name);
        let symbols = document_symbols(&source);

        let actual: Vec<SymbolSnapshot> = symbols
            .iter()
            .map(|s| SymbolSnapshot {
                name: s.name.clone(),
                kind: symbol_kind_str(s.kind),
            })
            .collect();

        if should_update() {
            let json = serde_json::to_string_pretty(&actual).unwrap();
            fs::write(snap_path, format!("{}\n", json)).unwrap();
            println!("Updated snapshot: {}", snap_path.display());
            continue;
        }

        let expected_json = fs::read_to_string(snap_path)
            .unwrap_or_else(|e| panic!("Missing snapshot {}: {}", snap_path.display(), e));
        let expected: Vec<SymbolSnapshot> = serde_json::from_str(&expected_json)
            .unwrap_or_else(|e| panic!("Bad JSON in {}: {}", snap_path.display(), e));

        assert_eq!(
            actual, expected,
            "Symbol mismatch for {}\nActual: {:#?}",
            karu_name, actual,
        );
    }
}

// ── Formatting snapshot tests ───────────────────────────────────────

#[test]
fn test_formatting_messy() {
    let messy = load_fixture("fmt_messy.karu");
    let expected = load_fixture("fmt_messy.formatted.karu");

    let formatted = format_source(&messy).expect("formatting should succeed");
    assert_eq!(
        formatted, expected,
        "Formatted output doesn't match expected.\nGot:\n{}\nExpected:\n{}",
        formatted, expected,
    );
}

#[test]
fn test_formatting_idempotent_on_valid_files() {
    let dir = fixtures_dir();
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();

        // Only test files that start with "valid_" or "schema_"
        if !name.starts_with("valid_") && !name.starts_with("schema_") {
            continue;
        }
        if path.extension().map_or(true, |e| e != "karu") {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap();
        match format_source(&source) {
            Ok(formatted) => {
                assert_eq!(
                    formatted, source,
                    "File {} is not idempotently formatted",
                    name
                );
            }
            Err(e) => {
                panic!("Failed to format {}: {}", name, e);
            }
        }
    }
}

// ── Rule locations tests ────────────────────────────────────────────

#[test]
fn test_find_rule_locations_on_valid_files() {
    let dir = fixtures_dir();
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();

        if !name.starts_with("valid_") && !name.starts_with("schema_") {
            continue;
        }
        if path.extension().map_or(true, |e| e != "karu") {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap();
        let locations = find_rule_locations(&source);

        // Every valid file with rules should have at least one location
        if source.contains("allow ") || source.contains("deny ") {
            assert!(
                !locations.is_empty(),
                "No rule locations found for {}\nSource:\n{}",
                name,
                source,
            );
        }
    }
}

// ── Semantic tokens tests ───────────────────────────────────────────

#[test]
fn test_semantic_tokens_not_empty() {
    let dir = fixtures_dir();
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();

        if path.extension().map_or(true, |e| e != "karu") {
            continue;
        }
        // Skip formatted companion files
        if name.contains(".formatted.") {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap();
        let tokens = semantic_tokens(&source);

        assert!(
            !tokens.is_empty(),
            "semantic_tokens returned empty for {}",
            name,
        );
    }
}

// ── Schema-specific regression tests ────────────────────────────────

#[test]
fn test_schema_files_produce_no_diagnostics() {
    for name in &[
        "schema_basic.karu",
        "schema_with_assert.karu",
        "schema_with_tests.karu",
        "schema_full_namespace.karu",
        "schema_empty_mod.karu",
        "schema_field_access_valid.karu",
        "schema_abstract_trait.karu",
    ] {
        let source = load_fixture(name);
        let diagnostics = parse_diagnostics(&source);
        assert!(
            diagnostics.is_empty(),
            "Schema file {} should have 0 diagnostics, got {}:\n{:#?}",
            name,
            diagnostics.len(),
            diagnostics,
        );
    }
}

#[test]
fn test_schema_files_have_symbols() {
    for name in &[
        "schema_basic.karu",
        "schema_with_assert.karu",
        "schema_with_tests.karu",
        "schema_full_namespace.karu",
        "schema_empty_mod.karu",
        "schema_field_access_valid.karu",
        "schema_abstract_trait.karu",
    ] {
        let source = load_fixture(name);
        let symbols = document_symbols(&source);
        assert!(
            !symbols.is_empty(),
            "Schema file {} should have symbols in outline",
            name,
        );
    }
}

#[test]
fn test_schema_files_have_rule_locations() {
    for name in &[
        "schema_basic.karu",
        "schema_with_assert.karu",
        "schema_with_tests.karu",
        "schema_full_namespace.karu",
        "schema_empty_mod.karu",
        "schema_field_access_valid.karu",
        "schema_abstract_trait.karu",
    ] {
        let source = load_fixture(name);
        let locations = find_rule_locations(&source);
        assert!(
            !locations.is_empty(),
            "Schema file {} should have rule locations",
            name,
        );
    }
}
