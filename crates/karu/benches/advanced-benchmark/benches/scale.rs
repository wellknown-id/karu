//! Scale benchmark: realistic authorization workload.
//!
//! Pre-seed the database first:
//!   cargo run --release --bin seed
//!
//! Then run benchmarks:
//!   cargo bench -- scale

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::hint::black_box;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Policies
// ---------------------------------------------------------------------------

const KARU_READ_POLICY: &str = r#"
allow read_public if
    principal.active == true and
    resource.public == true;

allow read_own if
    principal.active == true and
    principal.id == resource.owner_id;

allow read_shared if
    principal.active == true and
    shared == true;

allow read_org if
    principal.active == true and
    same_org == true and
    principal.role in ["admin", "editor"];
"#;

const KARU_DELETE_POLICY: &str = r#"
allow delete_own if
    principal.active == true and
    principal.id == resource.owner_id;

allow delete_admin if
    principal.active == true and
    principal.role == "admin" and
    same_org == true;
"#;

const CEDAR_READ_POLICY: &str = r#"
permit(principal, action == Action::"read", resource)
when { principal.active == true && resource.public == true };

permit(principal, action == Action::"read", resource)
when { principal.active == true && principal.id == resource.owner_id };

permit(principal, action == Action::"read", resource)
when { principal.active == true && context.shared == true };

permit(principal, action == Action::"read", resource)
when { principal.active == true && context.same_org == true && principal.role == "admin" };

permit(principal, action == Action::"read", resource)
when { principal.active == true && context.same_org == true && principal.role == "editor" };
"#;

const CEDAR_DELETE_POLICY: &str = r#"
permit(principal, action == Action::"delete", resource)
when { principal.active == true && principal.id == resource.owner_id };

permit(principal, action == Action::"delete", resource)
when { principal.active == true && principal.role == "admin" && context.same_org == true };
"#;

// ---------------------------------------------------------------------------
// Query types
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ReadQuery {
    label: &'static str,
    karu_input: Value,
    cedar_input: Value,
    expected_allow: bool,
}

#[derive(Clone)]
struct DeleteQuery {
    label: &'static str,
    karu_input: Value,
    cedar_input: Value,
    expected_allow: bool,
}

// ---------------------------------------------------------------------------
// Data loading
// ---------------------------------------------------------------------------

struct UserRow {
    id: i64,
    role: String,
    org_id: i64,
    active: bool,
}

struct FileRow {
    id: i64,
    owner_id: i64,
    public: bool,
}

fn db_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("scale.db")
}

fn load_users(conn: &Connection, limit: usize) -> Vec<UserRow> {
    let mut stmt = conn
        .prepare("SELECT id, role, org_id, active FROM users LIMIT ?1")
        .unwrap();
    stmt.query_map([limit as i64], |row| {
        Ok(UserRow {
            id: row.get(0)?,
            role: row.get(1)?,
            org_id: row.get(2)?,
            active: row.get::<_, i32>(3)? != 0,
        })
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}

fn load_files_for_user(conn: &Connection, user_id: i64, limit: usize) -> Vec<FileRow> {
    let mut stmt = conn
        .prepare("SELECT id, owner_id, public FROM files WHERE owner_id = ?1 LIMIT ?2")
        .unwrap();
    stmt.query_map([user_id, limit as i64], |row| {
        Ok(FileRow {
            id: row.get(0)?,
            owner_id: row.get(1)?,
            public: row.get::<_, i32>(2)? != 0,
        })
    })
    .unwrap()
    .map(|r| r.unwrap())
    .collect()
}

fn get_owner_org(conn: &Connection, owner_id: i64) -> i64 {
    conn.query_row("SELECT org_id FROM users WHERE id = ?1", [owner_id], |r| {
        r.get(0)
    })
    .unwrap()
}

fn find_shared_file_for_user(conn: &Connection, user_id: i64) -> Option<(i64, i64)> {
    conn.query_row(
        "SELECT s.file_id, f.owner_id FROM shares s
         JOIN files f ON f.id = s.file_id
         WHERE s.user_id = ?1 AND f.public = 0
         LIMIT 1",
        [user_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .ok()
}

fn find_unshared_private_file(
    conn: &Connection,
    user_id: i64,
    not_owner: i64,
) -> Option<(i64, i64)> {
    conn.query_row(
        "SELECT f.id, f.owner_id FROM files f
         WHERE f.owner_id = ?1 AND f.public = 0
         AND f.id NOT IN (SELECT file_id FROM shares WHERE user_id = ?2)
         LIMIT 1",
        [not_owner, user_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .ok()
}

// ---------------------------------------------------------------------------
// Query generation
// ---------------------------------------------------------------------------

fn build_read_queries(conn: &Connection, count: usize) -> Vec<ReadQuery> {
    let mut rng = StdRng::seed_from_u64(123);
    let users = load_users(conn, 10_000);
    let mut queries = Vec::with_capacity(count * 5);

    let active_users: Vec<&UserRow> = users.iter().filter(|u| u.active).collect();
    let disabled_users: Vec<&UserRow> = users.iter().filter(|u| !u.active).collect();

    for _ in 0..count {
        // read_own: user reading their own file
        let user = active_users[rng.gen_range(0..active_users.len())];
        let files = load_files_for_user(conn, user.id, 1);
        if let Some(file) = files.first() {
            let owner_org = user.org_id;
            let same_org = user.org_id == owner_org;
            let shared = false; // own file, doesn't matter
            queries.push(ReadQuery {
                label: "read_own",
                karu_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id, "public": file.public},
                    "shared": shared,
                    "same_org": same_org,
                }),
                cedar_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id, "public": file.public},
                    "context": {"shared": shared, "same_org": same_org},
                }),
                expected_allow: true,
            });
        }

        // read_public: user reading someone else's public file
        let other_user = active_users[rng.gen_range(0..active_users.len())];
        let other_files = load_files_for_user(conn, other_user.id, 10);
        if let Some(file) = other_files.iter().find(|f| f.public) {
            let owner_org = get_owner_org(conn, file.owner_id);
            let same_org = user.org_id == owner_org;
            queries.push(ReadQuery {
                label: "read_public",
                karu_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id, "public": true},
                    "shared": false,
                    "same_org": same_org,
                }),
                cedar_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id, "public": true},
                    "context": {"shared": false, "same_org": same_org},
                }),
                expected_allow: true,
            });
        }

        // read_shared: user reading a private file shared with them
        if let Some((file_id, owner_id)) = find_shared_file_for_user(conn, user.id) {
            let owner_org = get_owner_org(conn, owner_id);
            let same_org = user.org_id == owner_org;
            queries.push(ReadQuery {
                label: "read_shared",
                karu_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file_id, "owner_id": owner_id, "public": false},
                    "shared": true,
                    "same_org": same_org,
                }),
                cedar_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file_id, "owner_id": owner_id, "public": false},
                    "context": {"shared": true, "same_org": same_org},
                }),
                expected_allow: true,
            });
        }

        // read_denied: user reading a private file NOT shared with them, not their own
        let deny_user_idx = rng.gen_range(0..active_users.len());
        let deny_owner_idx = (deny_user_idx + 1) % active_users.len();
        let deny_user = active_users[deny_user_idx];
        let deny_owner = active_users[deny_owner_idx];
        if let Some((file_id, owner_id)) =
            find_unshared_private_file(conn, deny_user.id, deny_owner.id)
        {
            let owner_org = get_owner_org(conn, owner_id);
            let same_org = deny_user.org_id == owner_org;
            // Only denied if not same org or not admin/editor
            let should_allow =
                same_org && (deny_user.role == "admin" || deny_user.role == "editor");
            queries.push(ReadQuery {
                label: if should_allow { "read_org" } else { "read_denied" },
                karu_input: json!({
                    "principal": {"id": deny_user.id, "active": true, "role": deny_user.role, "org_id": deny_user.org_id},
                    "resource": {"id": file_id, "owner_id": owner_id, "public": false},
                    "shared": false,
                    "same_org": same_org,
                }),
                cedar_input: json!({
                    "principal": {"id": deny_user.id, "active": true, "role": deny_user.role, "org_id": deny_user.org_id},
                    "resource": {"id": file_id, "owner_id": owner_id, "public": false},
                    "context": {"shared": false, "same_org": same_org},
                }),
                expected_allow: should_allow,
            });
        }

        // read_disabled: disabled user trying to read
        if !disabled_users.is_empty() {
            let disabled = disabled_users[rng.gen_range(0..disabled_users.len())];
            let files = load_files_for_user(conn, disabled.id, 1);
            if let Some(file) = files.first() {
                queries.push(ReadQuery {
                    label: "read_disabled",
                    karu_input: json!({
                        "principal": {"id": disabled.id, "active": false, "role": disabled.role, "org_id": disabled.org_id},
                        "resource": {"id": file.id, "owner_id": file.owner_id, "public": file.public},
                        "shared": false,
                        "same_org": true,
                    }),
                    cedar_input: json!({
                        "principal": {"id": disabled.id, "active": false, "role": disabled.role, "org_id": disabled.org_id},
                        "resource": {"id": file.id, "owner_id": file.owner_id, "public": file.public},
                        "context": {"shared": false, "same_org": true},
                    }),
                    expected_allow: false,
                });
            }
        }
    }

    queries
}

fn build_delete_queries(conn: &Connection, count: usize) -> Vec<DeleteQuery> {
    let mut rng = StdRng::seed_from_u64(456);
    let users = load_users(conn, 10_000);
    let mut queries = Vec::with_capacity(count * 2);

    let active_users: Vec<&UserRow> = users.iter().filter(|u| u.active).collect();

    for _ in 0..count {
        // delete_own: user deleting their own file
        let user = active_users[rng.gen_range(0..active_users.len())];
        let files = load_files_for_user(conn, user.id, 1);
        if let Some(file) = files.first() {
            queries.push(DeleteQuery {
                label: "delete_own",
                karu_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id},
                    "same_org": true,
                }),
                cedar_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id},
                    "context": {"same_org": true},
                }),
                expected_allow: true,
            });
        }

        // delete_denied: user trying to delete someone else's file
        let idx1 = rng.gen_range(0..active_users.len());
        let idx2 = (idx1 + 1) % active_users.len();
        let user = active_users[idx1];
        let owner = active_users[idx2];
        let files = load_files_for_user(conn, owner.id, 1);
        if let Some(file) = files.first() {
            let owner_org = get_owner_org(conn, owner.id);
            let same_org = user.org_id == owner_org;
            let should_allow = user.role == "admin" && same_org;
            queries.push(DeleteQuery {
                label: if should_allow { "delete_admin" } else { "delete_denied" },
                karu_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id},
                    "same_org": same_org,
                }),
                cedar_input: json!({
                    "principal": {"id": user.id, "active": true, "role": user.role, "org_id": user.org_id},
                    "resource": {"id": file.id, "owner_id": file.owner_id},
                    "context": {"same_org": same_org},
                }),
                expected_allow: should_allow,
            });
        }
    }

    queries
}

// ---------------------------------------------------------------------------
// Cedar helpers
// ---------------------------------------------------------------------------

fn cedar_eval(
    authorizer: &cedar_policy::Authorizer,
    policy_set: &cedar_policy::PolicySet,
    input: &Value,
    action_name: &str,
) -> bool {
    // Build Cedar request from our JSON input
    let principal = cedar_policy::EntityUid::from_type_name_and_id(
        "User".parse().unwrap(),
        format!("{}", input["principal"]["id"]).parse().unwrap(),
    );
    let action = cedar_policy::EntityUid::from_type_name_and_id(
        "Action".parse().unwrap(),
        action_name.parse().unwrap(),
    );
    let resource = cedar_policy::EntityUid::from_type_name_and_id(
        "File".parse().unwrap(),
        format!("{}", input["resource"]["id"]).parse().unwrap(),
    );

    // Build principal entity with attributes
    let principal_attrs: std::collections::HashMap<String, cedar_policy::RestrictedExpression> =
        input["principal"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                let expr = match v {
                    Value::Bool(b) => cedar_policy::RestrictedExpression::new_bool(*b),
                    Value::Number(n) => {
                        cedar_policy::RestrictedExpression::new_long(n.as_i64().unwrap())
                    }
                    Value::String(s) => cedar_policy::RestrictedExpression::new_string(s.clone()),
                    _ => cedar_policy::RestrictedExpression::new_string(v.to_string()),
                };
                (k.clone(), expr)
            })
            .collect();

    let resource_attrs: std::collections::HashMap<String, cedar_policy::RestrictedExpression> =
        input["resource"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                let expr = match v {
                    Value::Bool(b) => cedar_policy::RestrictedExpression::new_bool(*b),
                    Value::Number(n) => {
                        cedar_policy::RestrictedExpression::new_long(n.as_i64().unwrap())
                    }
                    Value::String(s) => cedar_policy::RestrictedExpression::new_string(s.clone()),
                    _ => cedar_policy::RestrictedExpression::new_string(v.to_string()),
                };
                (k.clone(), expr)
            })
            .collect();

    // Build context from input["context"]
    let context_map: std::collections::HashMap<String, cedar_policy::RestrictedExpression> =
        if let Some(ctx) = input.get("context").and_then(|c| c.as_object()) {
            ctx.iter()
                .map(|(k, v)| {
                    let expr = match v {
                        Value::Bool(b) => cedar_policy::RestrictedExpression::new_bool(*b),
                        Value::Number(n) => {
                            cedar_policy::RestrictedExpression::new_long(n.as_i64().unwrap())
                        }
                        Value::String(s) => {
                            cedar_policy::RestrictedExpression::new_string(s.clone())
                        }
                        _ => cedar_policy::RestrictedExpression::new_string(v.to_string()),
                    };
                    (k.clone(), expr)
                })
                .collect()
        } else {
            std::collections::HashMap::new()
        };

    let context = cedar_policy::Context::from_pairs(context_map).unwrap();

    let principal_entity = cedar_policy::Entity::new(
        principal.clone(),
        principal_attrs,
        std::collections::HashSet::new(),
    )
    .unwrap();
    let resource_entity = cedar_policy::Entity::new(
        resource.clone(),
        resource_attrs,
        std::collections::HashSet::new(),
    )
    .unwrap();

    let entities =
        cedar_policy::Entities::from_entities([principal_entity, resource_entity], None).unwrap();

    let request = cedar_policy::Request::new(principal, action, resource, context, None).unwrap();

    let response = authorizer.is_authorized(&request, &policy_set, &entities);
    response.decision() == cedar_policy::Decision::Allow
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_karu_read(c: &mut Criterion) {
    let path = db_path();
    if !path.exists() {
        eprintln!(
            "Database not found at {}. Run: cargo run --release --bin seed",
            path.display()
        );
        return;
    }
    let conn = Connection::open(&path).unwrap();
    let queries = build_read_queries(&conn, 200);
    let policy = karu::compile(KARU_READ_POLICY).expect("Failed to compile Karu read policy");

    // Correctness check
    for q in &queries {
        let result = policy.evaluate(&q.karu_input);
        let got = result == karu::Effect::Allow;
        assert_eq!(
            got, q.expected_allow,
            "Karu read correctness failed for {}: input={}, expected={}, got={}",
            q.label, q.karu_input, q.expected_allow, got
        );
    }

    let mut group = c.benchmark_group("scale_karu_read");
    group.throughput(Throughput::Elements(queries.len() as u64));

    // Single-threaded
    group.bench_function("single_thread", |b| {
        b.iter(|| {
            for q in &queries {
                black_box(policy.evaluate(black_box(&q.karu_input)));
            }
        })
    });

    // Multi-threaded
    let queries_arc = Arc::new(queries.clone());
    let policy_arc = Arc::new(policy);

    for threads in [2, 4, 8, 16] {
        let qs = queries_arc.clone();
        let p = policy_arc.clone();
        group.bench_with_input(BenchmarkId::new("threads", threads), &threads, |b, &n| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .unwrap();
            b.iter(|| {
                pool.install(|| {
                    use rayon::prelude::*;
                    qs.par_iter().for_each(|q| {
                        black_box(p.evaluate(black_box(&q.karu_input)));
                    });
                })
            })
        });
    }
    group.finish();
}

fn bench_cedar_read(c: &mut Criterion) {
    let path = db_path();
    if !path.exists() {
        return;
    }
    let conn = Connection::open(&path).unwrap();
    let queries = build_read_queries(&conn, 200);

    let policy_set: cedar_policy::PolicySet = CEDAR_READ_POLICY
        .parse()
        .expect("Failed to parse Cedar read policy");
    let authorizer = cedar_policy::Authorizer::new();

    // Correctness check
    for q in &queries {
        let got = cedar_eval(&authorizer, &policy_set, &q.cedar_input, "read");
        assert_eq!(
            got, q.expected_allow,
            "Cedar read correctness failed for {}: expected={}, got={}",
            q.label, q.expected_allow, got
        );
    }

    let mut group = c.benchmark_group("scale_cedar_read");
    group.throughput(Throughput::Elements(queries.len() as u64));

    group.bench_function("single_thread", |b| {
        b.iter(|| {
            for q in &queries {
                black_box(cedar_eval(
                    &authorizer,
                    &policy_set,
                    black_box(&q.cedar_input),
                    "read",
                ));
            }
        })
    });

    let queries_arc = Arc::new(queries.clone());
    let policy_arc = Arc::new(policy_set);
    let auth_arc = Arc::new(authorizer);

    for threads in [2, 4, 8, 16] {
        let qs = queries_arc.clone();
        let p = policy_arc.clone();
        let a = auth_arc.clone();
        group.bench_with_input(BenchmarkId::new("threads", threads), &threads, |b, &n| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .unwrap();
            b.iter(|| {
                pool.install(|| {
                    use rayon::prelude::*;
                    qs.par_iter().for_each(|q| {
                        black_box(cedar_eval(&a, &p, black_box(&q.cedar_input), "read"));
                    });
                })
            })
        });
    }
    group.finish();
}

fn bench_karu_delete(c: &mut Criterion) {
    let path = db_path();
    if !path.exists() {
        return;
    }
    let conn = Connection::open(&path).unwrap();
    let queries = build_delete_queries(&conn, 200);
    let policy = karu::compile(KARU_DELETE_POLICY).expect("Failed to compile Karu delete policy");

    // Correctness check
    for q in &queries {
        let result = policy.evaluate(&q.karu_input);
        let got = result == karu::Effect::Allow;
        assert_eq!(
            got, q.expected_allow,
            "Karu delete correctness failed for {}: expected={}, got={}",
            q.label, q.expected_allow, got
        );
    }

    let mut group = c.benchmark_group("scale_karu_delete");
    group.throughput(Throughput::Elements(queries.len() as u64));

    group.bench_function("single_thread", |b| {
        b.iter(|| {
            for q in &queries {
                black_box(policy.evaluate(black_box(&q.karu_input)));
            }
        })
    });

    let queries_arc = Arc::new(queries.clone());
    let policy_arc = Arc::new(policy);

    for threads in [2, 4, 8, 16] {
        let qs = queries_arc.clone();
        let p = policy_arc.clone();
        group.bench_with_input(BenchmarkId::new("threads", threads), &threads, |b, &n| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .unwrap();
            b.iter(|| {
                pool.install(|| {
                    use rayon::prelude::*;
                    qs.par_iter().for_each(|q| {
                        black_box(p.evaluate(black_box(&q.karu_input)));
                    });
                })
            })
        });
    }
    group.finish();
}

fn bench_cedar_delete(c: &mut Criterion) {
    let path = db_path();
    if !path.exists() {
        return;
    }
    let conn = Connection::open(&path).unwrap();
    let queries = build_delete_queries(&conn, 200);

    let policy_set: cedar_policy::PolicySet = CEDAR_DELETE_POLICY
        .parse()
        .expect("Failed to parse Cedar delete policy");
    let authorizer = cedar_policy::Authorizer::new();

    // Correctness check
    for q in &queries {
        let got = cedar_eval(&authorizer, &policy_set, &q.cedar_input, "delete");
        assert_eq!(
            got, q.expected_allow,
            "Cedar delete correctness failed for {}: expected={}, got={}",
            q.label, q.expected_allow, got
        );
    }

    let mut group = c.benchmark_group("scale_cedar_delete");
    group.throughput(Throughput::Elements(queries.len() as u64));

    group.bench_function("single_thread", |b| {
        b.iter(|| {
            for q in &queries {
                black_box(cedar_eval(
                    &authorizer,
                    &policy_set,
                    black_box(&q.cedar_input),
                    "delete",
                ));
            }
        })
    });

    let queries_arc = Arc::new(queries.clone());
    let policy_arc = Arc::new(policy_set);
    let auth_arc = Arc::new(authorizer);

    for threads in [2, 4, 8, 16] {
        let qs = queries_arc.clone();
        let p = policy_arc.clone();
        let a = auth_arc.clone();
        group.bench_with_input(BenchmarkId::new("threads", threads), &threads, |b, &n| {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build()
                .unwrap();
            b.iter(|| {
                pool.install(|| {
                    use rayon::prelude::*;
                    qs.par_iter().for_each(|q| {
                        black_box(cedar_eval(&a, &p, black_box(&q.cedar_input), "delete"));
                    });
                })
            })
        });
    }
    group.finish();
}

criterion_group!(
    scale,
    bench_karu_read,
    bench_cedar_read,
    bench_karu_delete,
    bench_cedar_delete,
);
criterion_main!(scale);
