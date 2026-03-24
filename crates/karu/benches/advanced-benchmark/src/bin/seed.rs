//! Deterministic database seeder for scale benchmarks.
//!
//! Usage:
//!   cargo run --bin seed              # 10,000 users (quick, ~1.5M files)
//!   cargo run --bin seed -- --full    # 1,000,000 users (~150M files)

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rusqlite::{params, Connection};
use std::time::Instant;

const SEED: u64 = 42;
const NUM_ORGS: u32 = 500;

fn main() {
    let full = std::env::args().any(|a| a == "--full");
    let num_users: u32 = if full { 1_000_000 } else { 10_000 };

    let db_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    std::fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join(if full { "scale_full.db" } else { "scale.db" });

    // Remove existing DB for clean seed
    if db_path.exists() {
        std::fs::remove_file(&db_path).unwrap();
    }

    println!("Seeding {} users → {}", num_users, db_path.display());
    let start = Instant::now();

    let mut conn = Connection::open(&db_path).unwrap();

    // Performance pragmas
    conn.execute_batch(
        "PRAGMA journal_mode = OFF;
         PRAGMA synchronous = OFF;
         PRAGMA cache_size = -1048576;
         PRAGMA locking_mode = EXCLUSIVE;
         PRAGMA temp_store = MEMORY;",
    )
    .unwrap();

    create_schema(&conn);

    let mut rng = StdRng::seed_from_u64(SEED);

    // --- Insert users ---
    let t = Instant::now();
    insert_users(&mut conn, &mut rng, num_users);
    println!(
        "  Users:  {:>10} in {:.1}s",
        num_users,
        t.elapsed().as_secs_f64()
    );

    // --- Insert files + shares ---
    let t = Instant::now();
    let (file_count, share_count) = insert_files_and_shares(&mut conn, &mut rng, num_users);
    println!(
        "  Files:  {:>10} in {:.1}s",
        file_count,
        t.elapsed().as_secs_f64()
    );
    println!("  Shares: {:>10}", share_count);

    // --- Create indexes ---
    let t = Instant::now();
    create_indexes(&conn);
    println!("  Indexes created in {:.1}s", t.elapsed().as_secs_f64());

    let total = start.elapsed();
    let size_mb = std::fs::metadata(&db_path).unwrap().len() as f64 / 1_048_576.0;
    println!("\nDone in {:.1}s - {:.0} MB", total.as_secs_f64(), size_mb);
}

fn create_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE users (
            id       INTEGER PRIMARY KEY,
            username TEXT    NOT NULL,
            role     TEXT    NOT NULL,
            org_id   INTEGER NOT NULL,
            tier     TEXT    NOT NULL,
            active   INTEGER NOT NULL
        );

        CREATE TABLE files (
            id       INTEGER PRIMARY KEY,
            owner_id INTEGER NOT NULL,
            name     TEXT    NOT NULL,
            ftype    TEXT    NOT NULL,
            size_mb  INTEGER NOT NULL,
            public   INTEGER NOT NULL
        );

        CREATE TABLE shares (
            file_id  INTEGER NOT NULL,
            user_id  INTEGER NOT NULL,
            PRIMARY KEY (file_id, user_id)
        );",
    )
    .unwrap();
}

fn create_indexes(conn: &Connection) {
    conn.execute_batch(
        "CREATE INDEX idx_files_owner ON files(owner_id);
         CREATE INDEX idx_shares_file ON shares(file_id);
         CREATE INDEX idx_shares_user ON shares(user_id);
         CREATE INDEX idx_users_org   ON users(org_id);",
    )
    .unwrap();
}

fn pick_role(rng: &mut StdRng) -> &'static str {
    let r: f64 = rng.gen();
    if r < 0.01 {
        "admin"
    } else if r < 0.21 {
        "editor"
    } else {
        "viewer"
    }
}

fn pick_tier(rng: &mut StdRng) -> &'static str {
    let r: f64 = rng.gen();
    if r < 0.60 {
        "free"
    } else if r < 0.90 {
        "pro"
    } else {
        "enterprise"
    }
}

fn pick_file_type(rng: &mut StdRng) -> &'static str {
    let r: f64 = rng.gen();
    if r < 0.50 {
        "image"
    } else if r < 0.90 {
        "document"
    } else {
        "video"
    }
}

fn insert_users(conn: &mut Connection, rng: &mut StdRng, num_users: u32) {
    let tx = conn.transaction().unwrap();
    {
        let mut stmt = tx
            .prepare("INSERT INTO users (id, username, role, org_id, tier, active) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
            .unwrap();

        for i in 0..num_users {
            let id = i + 1; // 1-indexed
            let username = format!("user_{:07}", id);
            let role = pick_role(rng);
            let org_id = (id % NUM_ORGS) + 1;
            let tier = pick_tier(rng);
            let active = if id % 100 == 0 { 0i32 } else { 1i32 };

            stmt.execute(params![id, username, role, org_id, tier, active])
                .unwrap();
        }
    }
    tx.commit().unwrap();
}

fn insert_files_and_shares(conn: &mut Connection, rng: &mut StdRng, num_users: u32) -> (u64, u64) {
    let mut file_count: u64 = 0;
    let mut share_count: u64 = 0;
    let mut file_id: u64 = 0;

    // Process in batches for performance
    let batch_size = 1000;
    let mut user_start = 1u32;

    while user_start <= num_users {
        let user_end = (user_start + batch_size - 1).min(num_users);
        let tx = conn.transaction().unwrap();
        {
            let mut file_stmt = tx
                .prepare("INSERT INTO files (id, owner_id, name, ftype, size_mb, public) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
                .unwrap();
            let mut share_stmt = tx
                .prepare("INSERT INTO shares (file_id, user_id) VALUES (?1, ?2)")
                .unwrap();

            for user_id in user_start..=user_end {
                let num_files: u32 = rng.gen_range(100..=200);

                for _f in 0..num_files {
                    file_id += 1;
                    let name = format!("file_{}", file_id);
                    let ftype = pick_file_type(rng);
                    let size_mb: u32 = rng.gen_range(1..=500);
                    let is_public: bool = rng.gen::<f64>() < 0.70;

                    file_stmt
                        .execute(params![
                            file_id as i64,
                            user_id,
                            name,
                            ftype,
                            size_mb,
                            is_public as i32
                        ])
                        .unwrap();
                    file_count += 1;

                    // Private files: 50% chance of being shared
                    if !is_public && rng.gen::<f64>() < 0.50 {
                        let num_shares: u32 = rng.gen_range(3..=6);
                        for _ in 0..num_shares {
                            // Pick a random user (not the owner)
                            let mut shared_with = rng.gen_range(1..=num_users);
                            while shared_with == user_id {
                                shared_with = rng.gen_range(1..=num_users);
                            }
                            // Ignore duplicates (PRIMARY KEY constraint)
                            let _ = share_stmt.execute(params![file_id as i64, shared_with]);
                            share_count += 1;
                        }
                    }
                }
            }
        }
        tx.commit().unwrap();

        if user_start % 10000 == 1 || user_end == num_users {
            eprint!(
                "\r  Progress: {}/{} users ({:.0}%)",
                user_end,
                num_users,
                user_end as f64 / num_users as f64 * 100.0
            );
        }
        user_start = user_end + 1;
    }
    eprintln!();

    (file_count, share_count)
}
