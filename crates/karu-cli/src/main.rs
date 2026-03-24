//! Karu CLI - Policy evaluation and transpilation tool.

use clap::{Parser, Subcommand};
use karu::compiler::compile;
use karu::parser::Parser as KaruParser;
use karu::rule::Effect;
#[cfg(feature = "cedar")]
use karu::transpile::to_cedar;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "karu")]
#[command(about = "Karu policy engine - evaluate and transpile policies")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable strict mode (reject non-Cedar-compatible features)
    #[arg(long, global = true)]
    strict: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Evaluate a policy against JSON input
    Eval {
        /// Path to the Karu policy file
        policy: PathBuf,
        /// Path to the JSON input file
        input: PathBuf,
    },
    /// Transpile a Karu policy to Cedar syntax
    #[cfg(feature = "cedar")]
    Transpile {
        /// Path to the Karu policy file
        policy: PathBuf,
    },
    /// Check a Karu policy for syntax errors
    Check {
        /// Path to the Karu policy file
        policy: PathBuf,
    },
    /// Import a Cedar policy and convert to Karu syntax
    #[cfg(feature = "cedar")]
    Import {
        /// Path to the Cedar policy file
        policy: PathBuf,
    },
    /// Run inline tests declared in a Karu policy file
    Test {
        /// Path to the Karu policy file
        policy: PathBuf,
    },
    /// Format a Karu policy file
    Fmt {
        /// Path to the Karu policy file
        policy: PathBuf,
        /// Check if file is formatted (exit non-zero if not)
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Eval { policy, input } => {
            let policy_src = fs::read_to_string(&policy)
                .map_err(|e| format!("Failed to read policy file: {}", e))?;
            let input_src = fs::read_to_string(&input)
                .map_err(|e| format!("Failed to read input file: {}", e))?;

            // Check strict mode
            if cli.strict {
                check_strict(&policy_src)?;
            }

            let compiled = compile(&policy_src).map_err(|e| e.format_with_source(&policy_src))?;

            let json: serde_json::Value =
                serde_json::from_str(&input_src).map_err(|e| format!("Invalid JSON: {}", e))?;

            match compiled.evaluate(&json) {
                Effect::Allow => {
                    println!("ALLOW");
                    Ok(())
                }
                Effect::Deny => {
                    println!("DENY");
                    Ok(())
                }
            }
        }
        #[cfg(feature = "cedar")]
        Commands::Transpile { policy } => {
            let policy_src = fs::read_to_string(&policy)
                .map_err(|e| format!("Failed to read policy file: {}", e))?;

            // Check strict mode
            if cli.strict {
                check_strict(&policy_src)?;
            }

            let ast =
                KaruParser::parse(&policy_src).map_err(|e| e.format_with_source(&policy_src))?;

            let cedar = to_cedar(&ast).map_err(|e| format!("Transpile error: {}", e))?;

            println!("{}", cedar);
            Ok(())
        }
        Commands::Check { policy } => {
            let policy_src = fs::read_to_string(&policy)
                .map_err(|e| format!("Failed to read policy file: {}", e))?;

            // Check strict mode
            if cli.strict {
                check_strict(&policy_src)?;
            }

            let ast =
                KaruParser::parse(&policy_src).map_err(|e| e.format_with_source(&policy_src))?;

            // In strict mode, also verify it can be transpiled
            #[cfg(feature = "cedar")]
            if cli.strict {
                to_cedar(&ast).map_err(|e| format!("Not Cedar-compatible: {}", e))?;
            }

            println!("OK ({} rules)", ast.rules.len());
            Ok(())
        }
        #[cfg(feature = "cedar")]
        Commands::Import { policy } => {
            let cedar_src = fs::read_to_string(&policy)
                .map_err(|e| format!("Failed to read Cedar policy file: {}", e))?;

            let karu = karu::from_cedar_to_source(&cedar_src)
                .map_err(|e| format!("Import error: {}", e))?;

            println!("{}", karu);
            Ok(())
        }
        Commands::Test { policy } => {
            let policy_src = fs::read_to_string(&policy)
                .map_err(|e| format!("Failed to read policy file: {}", e))?;

            let program = KaruParser::parse_with_tests(&policy_src)
                .map_err(|e| e.format_with_source(&policy_src))?;

            if program.tests.is_empty() {
                println!("No tests found in {}", policy.display());
                return Ok(());
            }

            let compiled = compile(&policy_src).map_err(|e| e.format_with_source(&policy_src))?;

            let mut passed = 0;
            let mut failed = 0;

            for test in &program.tests {
                // Build the request JSON from test entities
                // For each entity (principal, resource, action), set:
                //   - top-level `kind` = id field value (for `principal == "alice"`)
                //   - `kind.field` = each field (for dotted path access)
                let mut flat = serde_json::Map::new();
                for entity in &test.entities {
                    let mut id_value = None;
                    for (key, value) in &entity.fields {
                        if key == "id" {
                            id_value = Some(value.clone());
                        }
                        // Dotted path: resource.type, principal.id, etc.
                        flat.insert(format!("{}.{}", entity.kind, key), value.clone());
                    }
                    // Top-level key = id value for simple equality
                    if let Some(id) = id_value {
                        flat.insert(entity.kind.clone(), id);
                    }
                }
                let input = serde_json::Value::Object(flat);

                let result = compiled.evaluate(&input);
                let expected = match &test.expected {
                    karu::ast::ExpectedOutcome::Simple(eff) => match eff {
                        karu::ast::EffectAst::Allow => Effect::Allow,
                        karu::ast::EffectAst::Deny => Effect::Deny,
                    },
                    karu::ast::ExpectedOutcome::PerRule(entries) => {
                        // Per-rule: check each named rule individually
                        let mut all_pass = true;
                        for (eff, rule_name) in entries {
                            let expected_eff = match eff {
                                karu::ast::EffectAst::Allow => Effect::Allow,
                                karu::ast::EffectAst::Deny => Effect::Deny,
                            };
                            if let Some(rule) = compiled.rules.iter().find(|r| r.name == *rule_name)
                            {
                                let rule_result = if rule.evaluate(&input).is_some() {
                                    rule.effect
                                } else {
                                    // Rule didn't match = no effect (treat as deny by default)
                                    Effect::Deny
                                };
                                if rule_result != expected_eff {
                                    println!(
                                        "  ✗ {} (rule '{}': expected {:?}, got {:?})",
                                        test.name, rule_name, expected_eff, rule_result
                                    );
                                    all_pass = false;
                                }
                            } else {
                                println!("  ✗ {} (rule '{}' not found)", test.name, rule_name);
                                all_pass = false;
                            }
                        }
                        if all_pass {
                            println!("  ✓ {}", test.name);
                            passed += 1;
                        } else {
                            failed += 1;
                        }
                        continue;
                    }
                };

                if result == expected {
                    println!("  ✓ {}", test.name);
                    passed += 1;
                } else {
                    println!(
                        "  ✗ {} (expected {:?}, got {:?})",
                        test.name, expected, result
                    );
                    failed += 1;
                }
            }

            println!();
            if failed == 0 {
                println!("{} tests passed", passed);
                Ok(())
            } else {
                Err(format!("{} passed, {} failed", passed, failed))
            }
        }
        Commands::Fmt { policy, check } => {
            let policy_src = fs::read_to_string(&policy)
                .map_err(|e| format!("Failed to read policy file: {}", e))?;

            let formatted = karu::format::format_source(&policy_src)?;

            if check {
                if formatted == policy_src {
                    println!("OK {}", policy.display());
                    Ok(())
                } else {
                    Err(format!("{} needs formatting", policy.display()))
                }
            } else if formatted == policy_src {
                println!("Already formatted: {}", policy.display());
                Ok(())
            } else {
                fs::write(&policy, &formatted).map_err(|e| format!("Failed to write: {}", e))?;
                println!("Formatted: {}", policy.display());
                Ok(())
            }
        }
    }
}

/// Check policy for strict mode compliance (no forall, no variables, no wildcards).
fn check_strict(source: &str) -> Result<(), String> {
    // Quick check for forbidden keywords
    if source.contains("forall ") {
        return Err("Strict mode: 'forall' is not Cedar-compatible".into());
    }
    // More thorough checks happen during transpilation
    Ok(())
}
