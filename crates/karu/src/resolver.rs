//! Import resolver for multi-file Karu projects.
//!
//! Resolves `import "path";` directives, merging imported programs into
//! a single [`Program`]. Validates two constraints:
//!
//! 1. **No circular imports** - the import graph must be a DAG.
//! 2. **Schema consistency** - a `use schema;` file can only import other
//!    `use schema;` files. Untyped files may freely import typed files.
//!
//! # Example
//!
//! ```rust,ignore
//! use karu::resolver::{resolve, FsSourceLoader};
//! use std::path::Path;
//!
//! let program = resolve(Path::new("policy.karu"), &FsSourceLoader)?;
//! ```

use crate::ast::Program;
use crate::parser::{ParseError, Parser};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// Error during import resolution.
#[derive(Debug, Clone)]
pub struct ResolveError {
    pub message: String,
    pub path: PathBuf,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for ResolveError {}

impl From<ParseError> for ResolveError {
    fn from(e: ParseError) -> Self {
        ResolveError {
            message: format!("Parse error: {}", e),
            path: PathBuf::new(),
        }
    }
}

/// Trait for loading source files. Allows testing without filesystem access.
pub trait SourceLoader {
    fn load(&self, path: &Path) -> Result<String, ResolveError>;
}

/// Filesystem-backed source loader.
pub struct FsSourceLoader;

impl SourceLoader for FsSourceLoader {
    fn load(&self, path: &Path) -> Result<String, ResolveError> {
        std::fs::read_to_string(path).map_err(|e| ResolveError {
            message: format!("Cannot read file: {}", e),
            path: path.to_path_buf(),
        })
    }
}

/// In-memory source loader for testing.
#[cfg(test)]
pub struct MemorySourceLoader {
    pub files: HashMap<PathBuf, String>,
}

#[cfg(test)]
impl SourceLoader for MemorySourceLoader {
    fn load(&self, path: &Path) -> Result<String, ResolveError> {
        self.files.get(path).cloned().ok_or_else(|| ResolveError {
            message: "File not found".to_string(),
            path: path.to_path_buf(),
        })
    }
}

/// Resolve all imports starting from `entry_path`, producing a merged program.
///
/// Recursively follows `import "...";` directives. Detects circular imports
/// and validates that schema/non-schema files are not mixed.
pub fn resolve(entry_path: &Path, loader: &dyn SourceLoader) -> Result<Program, ResolveError> {
    let canonical = normalize_path(entry_path);
    let mut visited = HashSet::new();
    let mut stack = Vec::new();
    let mut programs = HashMap::new();

    resolve_recursive(&canonical, loader, &mut visited, &mut stack, &mut programs)?;

    // Merge all programs into one
    let entry_program = programs.remove(&canonical).unwrap();
    let mut merged = entry_program;

    // Merge imported programs (order: depth-first, imports before importer)
    for (path, program) in programs {
        if path == canonical {
            continue;
        }
        merged.modules.extend(program.modules);
        merged.assertions.extend(program.assertions);
        merged.rules.extend(program.rules);
        merged.tests.extend(program.tests);
    }

    // Clear imports from the merged result (they've been resolved)
    merged.imports.clear();

    Ok(merged)
}

fn resolve_recursive(
    path: &Path,
    loader: &dyn SourceLoader,
    visited: &mut HashSet<PathBuf>,
    stack: &mut Vec<PathBuf>,
    programs: &mut HashMap<PathBuf, Program>,
) -> Result<(), ResolveError> {
    // Circular dependency check
    if stack.contains(&path.to_path_buf()) {
        let cycle: Vec<String> = stack
            .iter()
            .skip_while(|p| *p != &path.to_path_buf())
            .map(|p| p.display().to_string())
            .collect();
        return Err(ResolveError {
            message: format!(
                "Circular import detected: {} -> {}",
                cycle.join(" -> "),
                path.display()
            ),
            path: path.to_path_buf(),
        });
    }

    // Diamond import deduplication - already processed this file
    if visited.contains(&path.to_path_buf()) {
        return Ok(());
    }

    let source = loader.load(path)?;
    let program = Parser::parse(&source).map_err(|e| ResolveError {
        message: format!("Parse error: {}", e),
        path: path.to_path_buf(),
    })?;

    visited.insert(path.to_path_buf());
    stack.push(path.to_path_buf());

    // Check schema consistency for each import
    let parent_dir = path.parent().unwrap_or(Path::new("."));
    for import_path in &program.imports {
        let resolved = normalize_path(&parent_dir.join(import_path));

        // Load and parse the imported file to check its schema status
        let import_source = loader.load(&resolved)?;
        let import_program = Parser::parse(&import_source).map_err(|e| ResolveError {
            message: format!("Parse error: {}", e),
            path: resolved.clone(),
        })?;

        // Schema consistency: typed files can only import typed files.
        // Untyped files may import typed files freely.
        if program.use_schema && !import_program.use_schema {
            return Err(ResolveError {
                message: format!(
                    "Schema file cannot import non-schema file '{}'. \
                     When `use schema;` is active, all imports must also use `use schema;`.",
                    import_path
                ),
                path: path.to_path_buf(),
            });
        }

        // Store the parsed program so we don't re-parse
        if !visited.contains(&resolved) {
            programs.insert(resolved.clone(), import_program);
            // But we still need to recurse to resolve transitive imports
            // We re-insert after recursion if needed
        }

        resolve_recursive(&resolved, loader, visited, stack, programs)?;
    }

    stack.pop();

    // Store the parsed program
    programs.entry(path.to_path_buf()).or_insert(program);

    Ok(())
}

/// Normalize a path (resolve `.` and `..` without requiring the filesystem).
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_loader(files: Vec<(&str, &str)>) -> MemorySourceLoader {
        MemorySourceLoader {
            files: files
                .into_iter()
                .map(|(k, v)| (PathBuf::from(k), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn test_single_file_no_imports() {
        let loader = mem_loader(vec![("/policy.karu", r#"allow view if action == "view";"#)]);

        let program = resolve(Path::new("/policy.karu"), &loader).unwrap();
        assert_eq!(program.rules.len(), 1);
        assert!(program.imports.is_empty());
    }

    #[test]
    fn test_single_import() {
        let loader = mem_loader(vec![
            (
                "/main.karu",
                r#"
import "rules.karu";
allow view if action == "view";
"#,
            ),
            ("/rules.karu", r#"allow edit if action == "edit";"#),
        ]);

        let program = resolve(Path::new("/main.karu"), &loader).unwrap();
        assert_eq!(program.rules.len(), 2);
    }

    #[test]
    fn test_transitive_imports() {
        let loader = mem_loader(vec![
            (
                "/a.karu",
                r#"
import "b.karu";
allow a if action == "a";
"#,
            ),
            (
                "/b.karu",
                r#"
import "c.karu";
allow b if action == "b";
"#,
            ),
            ("/c.karu", r#"allow c if action == "c";"#),
        ]);

        let program = resolve(Path::new("/a.karu"), &loader).unwrap();
        assert_eq!(program.rules.len(), 3);
    }

    #[test]
    fn test_circular_import_detection() {
        let loader = mem_loader(vec![
            (
                "/a.karu",
                r#"
import "b.karu";
allow a if action == "a";
"#,
            ),
            (
                "/b.karu",
                r#"
import "a.karu";
allow b if action == "b";
"#,
            ),
        ]);

        let err = resolve(Path::new("/a.karu"), &loader).unwrap_err();
        assert!(
            err.message.contains("Circular import"),
            "Expected circular import error, got: {}",
            err.message
        );
    }

    #[test]
    fn test_circular_import_three_files() {
        let loader = mem_loader(vec![
            ("/a.karu", "import \"b.karu\";\nallow a;"),
            ("/b.karu", "import \"c.karu\";\nallow b;"),
            ("/c.karu", "import \"a.karu\";\nallow c;"),
        ]);

        let err = resolve(Path::new("/a.karu"), &loader).unwrap_err();
        assert!(
            err.message.contains("Circular import"),
            "Expected circular import error, got: {}",
            err.message
        );
    }

    #[test]
    fn test_schema_cannot_import_non_schema() {
        let loader = mem_loader(vec![
            (
                "/schema.karu",
                r#"
use schema;
import "rules.karu";
mod { actor User {}; };
"#,
            ),
            ("/rules.karu", r#"allow view if action == "view";"#),
        ]);

        let err = resolve(Path::new("/schema.karu"), &loader).unwrap_err();
        assert!(
            err.message.contains("Schema file cannot import non-schema"),
            "Expected schema constraint error, got: {}",
            err.message
        );
    }

    #[test]
    fn test_non_schema_can_import_schema() {
        let loader = mem_loader(vec![
            (
                "/main.karu",
                r#"
import "schema.karu";
allow view if action == "view";
"#,
            ),
            (
                "/schema.karu",
                r#"
use schema;
mod { actor User {}; };
allow edit;
"#,
            ),
        ]);

        let program = resolve(Path::new("/main.karu"), &loader).unwrap();
        assert_eq!(program.rules.len(), 2);
        // The imported schema modules should be merged in
        assert!(!program.modules.is_empty());
    }

    #[test]
    fn test_schema_can_import_schema() {
        let loader = mem_loader(vec![
            (
                "/main.karu",
                r#"
use schema;
import "types.karu";
mod { resource Document {}; };
allow view;
"#,
            ),
            (
                "/types.karu",
                r#"
use schema;
mod { actor User {}; };
"#,
            ),
        ]);

        let program = resolve(Path::new("/main.karu"), &loader).unwrap();
        assert!(program.use_schema);
        assert!(program.modules.len() >= 2);
    }

    #[test]
    fn test_diamond_import_deduplication() {
        // A imports B and C; both B and C import D
        // D's rules should only appear once
        let loader = mem_loader(vec![
            (
                "/a.karu",
                "import \"b.karu\";\nimport \"c.karu\";\nallow a;",
            ),
            ("/b.karu", "import \"d.karu\";\nallow b;"),
            ("/c.karu", "import \"d.karu\";\nallow c;"),
            ("/d.karu", "allow d;"),
        ]);

        let program = resolve(Path::new("/a.karu"), &loader).unwrap();
        // Should have a, b, c, d - each once
        let rule_names: Vec<&str> = program.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            rule_names.iter().filter(|&&n| n == "d").count(),
            1,
            "Rule 'd' should appear exactly once, but found: {:?}",
            rule_names
        );
        assert_eq!(program.rules.len(), 4);
    }

    #[test]
    fn test_import_not_found() {
        let loader = mem_loader(vec![(
            "/main.karu",
            "import \"missing.karu\";\nallow view;",
        )]);

        let err = resolve(Path::new("/main.karu"), &loader).unwrap_err();
        assert!(
            err.message.contains("not found") || err.message.contains("Cannot read"),
            "Expected file not found error, got: {}",
            err.message
        );
    }

    #[test]
    fn test_relative_path_resolution() {
        let loader = mem_loader(vec![
            (
                "/project/main.karu",
                "import \"lib/rules.karu\";\nallow main;",
            ),
            ("/project/lib/rules.karu", "allow lib_rule;"),
        ]);

        let program = resolve(Path::new("/project/main.karu"), &loader).unwrap();
        assert_eq!(program.rules.len(), 2);
    }
}
