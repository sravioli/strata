//! Spike #100: sqlx against tokio-postgres on the four query shapes of tech design 0049.
//! Usage: pg-driver-bench <sqlx|tp|tp-pipe> <read|insert|status|search> <url> <admin-url> <secs> <path-label>
//! Prints one JSON line: transactions each second, p50 and p99 latency of one tenant transaction.

use std::{sync::Arc, time::{Duration, Instant}};

use chrono::{DateTime, Utc};
use deadpool_postgres::{ManagerConfig, RecyclingMethod};
use uuid::Uuid;

type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

const WARMUP: Duration = Duration::from_secs(2);
const PARTS: usize = 800;

// The tenant transaction of 0003: BEGIN, set_config, the query, COMMIT.
const SET: &str = "select set_config('app.tenant_id', $1, true)";
const READ: &str = "select state from revisions where tenant_id = $1 and document_id = $2 and revision = 1";
const STATUS: &str = "select document_id, revision, state, status_at from revisions \
    where tenant_id = $1 and (document_id, revision) in (select * from unnest($2::uuid[], $3::int4[]))";
const INSERT: &str = "with d as (insert into documents (tenant_id, last_revision, ready_revision) \
      values ($1, 1, 1) returning tenant_id, document_id), \
    r as (insert into revisions (tenant_id, document_id, revision, state, file_name, content_type, size) \
      select tenant_id, document_id, 1, 'Ready', 'bench.pdf', 'application/pdf', 1 from d \
      returning tenant_id, document_id, revision) \
    insert into text_parts (tenant_id, document_id, revision, part, location_kind, location, text) \
    select r.tenant_id, r.document_id, r.revision, u.part, 'page', u.part, u.text \
    from r, unnest($2::int4[], $3::text[]) as u(part, text)";
const SEARCH: &str = "select document_id, revision, score, location_kind, location from ( \
      select distinct on (document_id) document_id, revision, \
        ts_rank(tsv, websearch_to_tsquery('simple', $2)) as score, location_kind, location \
      from text_parts where tenant_id = $1 and tsv @@ websearch_to_tsquery('simple', $2) \
      order by document_id, score desc) as best \
    order by score desc limit $3";

#[derive(Clone, Copy, PartialEq)]
enum Shape { Read, Insert, Status, Search }

enum Lib { Sqlx(sqlx::PgPool), Tp(deadpool_postgres::Pool, bool) }

struct Tenant { id: Uuid, id_text: String, docs: Vec<Uuid> }

struct Data { tenants: Vec<Tenant>, part_no: Vec<i32>, part_text: Vec<String> }

struct Rng(u64);
impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

// Each shape returns a count of decoded rows, so the decode work cannot be skipped.
async fn once(lib: &Lib, shape: Shape, d: &Data, rng: &mut Rng) -> Res<usize> {
    let t = &d.tenants[rng.next(d.tenants.len())];
    let doc = t.docs[rng.next(t.docs.len())];
    let batch: Vec<Uuid> = (0..100).map(|_| t.docs[rng.next(t.docs.len())]).collect();
    let revs = vec![1i32; batch.len()];
    let word = format!("w{}", 1 + rng.next(10_000));
    let limit = 20i64;
    match lib {
        Lib::Sqlx(pool) => {
            use sqlx::Row;
            // BENCH_PROBE=empty|set cuts the transaction short, to find where the round trips go.
            let probe = std::env::var("BENCH_PROBE").unwrap_or_default();
            let mut tx = pool.begin().await?;
            if probe == "empty" { tx.commit().await?; return Ok(0); }
            sqlx::query(SET).bind(&t.id_text).execute(&mut *tx).await?;
            if probe == "set" { tx.commit().await?; return Ok(0); }
            let n = match shape {
                Shape::Read => {
                    let r = sqlx::query(READ).bind(t.id).bind(doc).fetch_one(&mut *tx).await?;
                    let _: String = r.try_get(0)?;
                    1
                }
                Shape::Status => {
                    let rows = sqlx::query(STATUS).bind(t.id).bind(&batch).bind(&revs).fetch_all(&mut *tx).await?;
                    for r in &rows {
                        let _: (Uuid, i32, String, DateTime<Utc>) = (r.try_get(0)?, r.try_get(1)?, r.try_get(2)?, r.try_get(3)?);
                    }
                    rows.len()
                }
                Shape::Insert => {
                    sqlx::query(INSERT).bind(t.id).bind(&d.part_no).bind(&d.part_text).execute(&mut *tx).await?.rows_affected() as usize
                }
                Shape::Search => {
                    let rows = sqlx::query(SEARCH).bind(t.id).bind(&word).bind(limit).fetch_all(&mut *tx).await?;
                    for r in &rows {
                        let _: (Uuid, i32, f32, String, i32) = (r.try_get(0)?, r.try_get(1)?, r.try_get(2)?, r.try_get(3)?, r.try_get(4)?);
                    }
                    rows.len()
                }
            };
            tx.commit().await?;
            Ok(n)
        }
        Lib::Tp(pool, pipe) => {
            let c = pool.get().await?;
            let set = c.prepare_cached(SET).await?;
            let q = c.prepare_cached(match shape {
                Shape::Read => READ, Shape::Status => STATUS, Shape::Insert => INSERT, Shape::Search => SEARCH,
            }).await?;
            let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = match shape {
                Shape::Read => vec![&t.id, &doc],
                Shape::Status => vec![&t.id, &batch, &revs],
                Shape::Insert => vec![&t.id, &d.part_no, &d.part_text],
                Shape::Search => vec![&t.id, &word, &limit],
            };
            let set_params: [&(dyn tokio_postgres::types::ToSql + Sync); 1] = [&t.id_text];
            let rows = if *pipe {
                // The statements are prepared, so each future sends its request on the first poll:
                // the four requests leave in one write, and the client waits one round trip.
                let (_, _, rows, _) = tokio::try_join!(
                    c.batch_execute("BEGIN"),
                    c.query_one(&set, &set_params),
                    c.query(&q, &params),
                    c.batch_execute("COMMIT"),
                )?;
                rows
            } else {
                let probe = std::env::var("BENCH_PROBE").unwrap_or_default();
                c.batch_execute("BEGIN").await?;
                if probe == "empty" { c.batch_execute("COMMIT").await?; return Ok(0); }
                c.query_one(&set, &set_params).await?;
                if probe == "set" { c.batch_execute("COMMIT").await?; return Ok(0); }
                let rows = c.query(&q, &params).await?;
                c.batch_execute("COMMIT").await?;
                rows
            };
            match shape {
                Shape::Read => { let _: String = rows[0].try_get(0)?; }
                Shape::Status => for r in &rows {
                    let _: (Uuid, i32, String, DateTime<Utc>) = (r.try_get(0)?, r.try_get(1)?, r.try_get(2)?, r.try_get(3)?);
                },
                Shape::Search => for r in &rows {
                    let _: (Uuid, i32, f32, String, i32) = (r.try_get(0)?, r.try_get(1)?, r.try_get(2)?, r.try_get(3)?, r.try_get(4)?);
                },
                Shape::Insert => {}
            }
            Ok(rows.len())
        }
    }
}

async fn load(admin: &str) -> Res<Data> {
    let (c, conn) = tokio_postgres::connect(admin, tokio_postgres::NoTls).await?;
    tokio::spawn(conn);
    let rows = c.query("select tenant_id, array_agg(document_id) from revisions \
        where file_name = 'seed.pdf' group by tenant_id order by tenant_id", &[]).await?;
    let tenants = rows.iter().map(|r| {
        let id: Uuid = r.get(0);
        Tenant { id, id_text: id.to_string(), docs: r.get(1) }
    }).collect();
    let mut rng = Rng(42);
    let part_text = (0..PARTS).map(|_| (0..300).map(|_| format!("w{}", 1 + rng.next(10_000))).collect::<Vec<_>>().join(" ")).collect();
    Ok(Data { tenants, part_no: (1..=PARTS as i32).collect(), part_text })
}

#[tokio::main]
async fn main() -> Res<()> {
    // BENCH_POOL and BENCH_TASKS exist for the round-trip count (1 and 1); the runs use 16 and 64.
    let env = |k: &str, d: usize| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let (pool_size, tasks_n) = (env("BENCH_POOL", 16) as u32, env("BENCH_TASKS", 64));
    let a: Vec<String> = std::env::args().collect();
    let [_, lib_name, shape_name, url, admin, secs, path] = &a[..] else {
        return Err("usage: pg-driver-bench <sqlx|tp|tp-pipe> <read|insert|status|search> <url> <admin-url> <secs> <path>".into());
    };
    let shape = match shape_name.as_str() {
        "read" => Shape::Read, "insert" => Shape::Insert, "status" => Shape::Status, "search" => Shape::Search,
        s => return Err(format!("unknown shape {s}").into()),
    };
    let lib = Arc::new(match lib_name.as_str() {
        // test_before_acquire(false) matches RecyclingMethod::Fast of deadpool: no ping on each checkout.
        "sqlx" => Lib::Sqlx(sqlx::postgres::PgPoolOptions::new()
            .max_connections(pool_size).min_connections(pool_size).test_before_acquire(false).connect(url).await?),
        "tp" | "tp-pipe" => {
            let mut cfg = deadpool_postgres::Config::new();
            cfg.url = Some(url.clone());
            cfg.manager = Some(ManagerConfig { recycling_method: RecyclingMethod::Fast });
            cfg.pool = Some(deadpool_postgres::PoolConfig::new(pool_size as usize));
            Lib::Tp(cfg.create_pool(Some(deadpool_postgres::Runtime::Tokio1), tokio_postgres::NoTls)?, lib_name == "tp-pipe")
        }
        l => return Err(format!("unknown lib {l}").into()),
    });
    let data = Arc::new(load(admin).await?);
    if data.tenants.is_empty() { return Err("no seed data".into()); }

    let start = Instant::now();
    let measure_from = start + WARMUP;
    let end = measure_from + Duration::from_secs(secs.parse()?);
    let mut tasks = Vec::new();
    for i in 0..tasks_n {
        let (lib, data) = (lib.clone(), data.clone());
        tasks.push(tokio::spawn(async move {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1));
            let mut lat = Vec::new();
            loop {
                let t0 = Instant::now();
                if t0 >= end { break; }
                // The first error ends the run: a connection with an aborted transaction must not go back to the pool.
                if let Err(e) = once(&lib, shape, &data, &mut rng).await { eprintln!("error: {e:?}"); std::process::exit(1); }
                if t0 >= measure_from { lat.push(t0.elapsed().as_micros() as u32); }
            }
            Res::Ok(lat)
        }));
    }
    let mut lat = Vec::new();
    for t in tasks { lat.extend(t.await??); }
    lat.sort_unstable();
    let pct = |p: f64| lat[((lat.len() as f64 * p) as usize).min(lat.len() - 1)];
    let measured = (end - measure_from).as_secs_f64();
    println!("{{\"path\":\"{path}\",\"lib\":\"{lib_name}\",\"shape\":\"{shape_name}\",\"n\":{},\"tps\":{:.1},\"p50_us\":{},\"p99_us\":{}}}",
        lat.len(), lat.len() as f64 / measured, pct(0.50), pct(0.99));
    Ok(())
}
