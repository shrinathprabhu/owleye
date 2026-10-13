//! Local, synthetic SQLite comparison; never opens the configured application DB.
//! Run: cargo run --locked -p owleye-api --example catalog_benchmark
use owleye_api::storage::sqlite::{connect, record_event_catalog_batch, CatalogEvent};
use sqlx::SqlitePool;
use std::time::{Duration, Instant};

async fn previous_batch(pool: &SqlitePool, events: &[CatalogEvent<'_>]) -> anyhow::Result<()> {
    let mut transaction = pool.begin().await?;
    for event in events {
        sqlx::query(
            "INSERT INTO site_event_catalog(site_id,event_type,event_name,first_seen_at,last_seen_at,total_seen)
             VALUES(?,?,?,?,?,1) ON CONFLICT(site_id,event_type,event_name) DO UPDATE SET
             first_seen_at=MIN(site_event_catalog.first_seen_at,excluded.first_seen_at),
             last_seen_at=MAX(site_event_catalog.last_seen_at,excluded.last_seen_at),
             total_seen=site_event_catalog.total_seen+1",
        )
        .bind(event.site_id)
        .bind(event.event_type)
        .bind(event.event_name)
        .bind(event.occurred_at)
        .bind(event.occurred_at)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(())
}

type CatalogRow = (String, String, String, String, String, i64);
async fn contents(pool: &SqlitePool) -> anyhow::Result<Vec<CatalogRow>> {
    Ok(sqlx::query_as("SELECT site_id,event_type,event_name,first_seen_at,last_seen_at,total_seen FROM site_event_catalog ORDER BY site_id,event_type,event_name").fetch_all(pool).await?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let old = connect("sqlite::memory:").await?;
    let new = connect("sqlite::memory:").await?;
    for (label, size, groups, repeats) in [
        ("single event", 1, 1, 500),
        ("100 events / 1 name", 100, 1, 100),
        ("100 events / 10 names", 100, 10, 100),
        ("100 events / 100 names", 100, 100, 100),
    ] {
        let names = (0..groups)
            .map(|i| format!("event-{i:03}"))
            .collect::<Vec<_>>();
        let events = (0..size)
            .map(|i| CatalogEvent {
                site_id: "synthetic",
                event_type: "external",
                event_name: &names[i % groups],
                occurred_at: "2026-10-13 00:00:00.000",
            })
            .collect::<Vec<_>>();
        let mut samples = [Vec::new(), Vec::new()];
        for round in 0..5 {
            // Alternate order to reduce systematic warmup and scheduling bias.
            for variant in [round % 2, 1 - round % 2] {
                let pool = if variant == 0 { &old } else { &new };
                sqlx::query("DELETE FROM site_event_catalog")
                    .execute(pool)
                    .await?;
                let start = Instant::now();
                for _ in 0..repeats {
                    if variant == 0 {
                        previous_batch(pool, &events).await?;
                    } else {
                        record_event_catalog_batch(pool, &events).await?;
                    }
                }
                samples[variant].push(start.elapsed());
            }
            assert_eq!(contents(&old).await?, contents(&new).await?);
        }
        let median = |values: &mut Vec<Duration>| {
            values.sort();
            values[2].as_secs_f64() * 1000.0 / repeats as f64
        };
        let baseline = median(&mut samples[0]);
        let folded = median(&mut samples[1]);
        println!("{label}: previous={baseline:.3}ms/batch folded={folded:.3}ms/batch ratio={:.2}x; identical rows", baseline / folded);
    }
    Ok(())
}
