//! End-to-end scenario test for M1.
//!
//! Walks the exact demo flow the milestone is meant to support: log a handful
//! of jobs by hand, exercise the list/search/filter paths, edit one through
//! `patch_job`, delete another, then verify the truck rollup is still
//! consistent with what's left in the sessions and jobs tables.
//!
//! This is a regression guard — every behaviour is also covered by a focused
//! unit test, but having one scripted scenario that runs the whole path makes
//! it obvious when something further down the stack starts misreporting
//! totals.

use chrono::{TimeZone, Utc};
use projecttrucks_lib::commands::jobs::{fetch_job, fetch_jobs, insert_job, patch_job, remove_job};
use projecttrucks_lib::commands::trucks::fetch_trucks;
use projecttrucks_lib::db::models::{JobFilter, JobPatch, JobSource, JobStatus, NewJob};
use projecttrucks_lib::db::rollups::truck_id;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;

async fn fresh_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

#[allow(clippy::too_many_arguments)]
fn manual_job(
    make: &str,
    model: &str,
    source: &str,
    dest: &str,
    cargo: &str,
    distance: f64,
    income: i64,
    status: JobStatus,
    day_of_month: u32,
) -> NewJob {
    NewJob {
        session_id: None,
        truck_make: Some(make.into()),
        truck_model: Some(model.into()),
        source_city: source.into(),
        source_company: format!("{} Logistics", source),
        dest_city: dest.into(),
        dest_company: format!("{} Freight", dest),
        cargo_id: cargo.to_lowercase(),
        cargo_name: cargo.into(),
        cargo_mass_kg: 18_000.0,
        declared_income: income,
        distance_km: distance,
        duration_sec: (distance * 60.0) as i64, // ≈ 60 km/h
        fuel_used_l: None,
        damage_delta: None,
        started_at: Utc
            .with_ymd_and_hms(2026, 5, day_of_month, 8, 0, 0)
            .unwrap(),
        completed_at: Utc
            .with_ymd_and_hms(2026, 5, day_of_month, 12, 0, 0)
            .unwrap(),
        status,
        source: JobSource::Manual,
        notes: None,
    }
}

#[tokio::test]
async fn m1_demo_flow_end_to_end() {
    let pool = fresh_pool().await;

    // Step 1 — log seven manual jobs spread across two trucks, three statuses,
    // and a date range that we can later filter on.
    let inputs = vec![
        manual_job(
            "Scania",
            "R 730",
            "Hamburg",
            "Berlin",
            "Containers",
            290.0,
            4_250,
            JobStatus::Delivered,
            1,
        ),
        manual_job(
            "Scania",
            "R 730",
            "Berlin",
            "Munich",
            "Steel coils",
            585.0,
            7_800,
            JobStatus::Delivered,
            2,
        ),
        manual_job(
            "Scania",
            "R 730",
            "Munich",
            "Vienna",
            "Logs",
            435.0,
            5_200,
            JobStatus::Late,
            3,
        ),
        manual_job(
            "Volvo",
            "FH 540",
            "Vienna",
            "Prague",
            "Furniture",
            330.0,
            4_100,
            JobStatus::Delivered,
            4,
        ),
        manual_job(
            "Volvo",
            "FH 540",
            "Prague",
            "Warsaw",
            "Cement",
            695.0,
            8_400,
            JobStatus::Delivered,
            5,
        ),
        manual_job(
            "Volvo",
            "FH 540",
            "Warsaw",
            "Berlin",
            "Glass",
            575.0,
            6_900,
            JobStatus::Cancelled,
            6,
        ),
        manual_job(
            "Scania",
            "R 730",
            "Berlin",
            "Hamburg",
            "Containers",
            295.0,
            4_300,
            JobStatus::Delivered,
            7,
        ),
    ];
    let mut ids = Vec::new();
    for input in inputs {
        let job = insert_job(&pool, input).await.unwrap();
        ids.push(job.id);
    }

    // Step 2 — table view should show all seven, newest first.
    let listed = fetch_jobs(&pool, JobFilter::default()).await.unwrap();
    assert_eq!(listed.total, 7);
    assert_eq!(listed.items.len(), 7);
    // First row is the May 7 trip.
    assert_eq!(
        listed.items[0].completed_at,
        Utc.with_ymd_and_hms(2026, 5, 7, 12, 0, 0).unwrap()
    );

    // Step 3 — filter by status: only Delivered rows.
    let delivered = fetch_jobs(
        &pool,
        JobFilter {
            status: Some(JobStatus::Delivered),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(delivered.total, 5);
    assert!(delivered
        .items
        .iter()
        .all(|j| j.status == JobStatus::Delivered));

    // Step 4 — search across city and cargo.
    let containers = fetch_jobs(
        &pool,
        JobFilter {
            search: Some("contain".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(containers.total, 2);

    let berlin = fetch_jobs(
        &pool,
        JobFilter {
            search: Some("Berlin".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    // Berlin appears as source on May 2, 7 and dest on May 1, 6 → four hits.
    assert_eq!(berlin.total, 4);

    // Step 5 — narrow by date range (May 3–5 inclusive).
    let mid_range = fetch_jobs(
        &pool,
        JobFilter {
            from: Some(Utc.with_ymd_and_hms(2026, 5, 3, 0, 0, 0).unwrap()),
            to: Some(Utc.with_ymd_and_hms(2026, 5, 6, 0, 0, 0).unwrap()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(mid_range.total, 3);

    // Step 6 — pagination.
    let page1 = fetch_jobs(
        &pool,
        JobFilter {
            limit: Some(3),
            offset: Some(0),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let page2 = fetch_jobs(
        &pool,
        JobFilter {
            limit: Some(3),
            offset: Some(3),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(page1.items.len(), 3);
    assert_eq!(page2.items.len(), 3);
    // No overlap between pages.
    let page1_ids: Vec<_> = page1.items.iter().map(|j| j.id.as_str()).collect();
    assert!(page2
        .items
        .iter()
        .all(|j| !page1_ids.contains(&j.id.as_str())));

    // Step 7 — open one job (the third one) and patch notes + flagged via
    // patch_job. Verify it persisted.
    let third = &ids[2];
    let patched = patch_job(
        &pool,
        third,
        JobPatch {
            notes: Some("Heavier than declared".into()),
            flagged: Some(true),
            flag_reason: Some("Mass check".into()),
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(patched.notes.as_deref(), Some("Heavier than declared"));
    assert!(patched.flagged);
    assert_eq!(patched.flag_reason.as_deref(), Some("Mass check"));
    // Round-trip through fetch_job to make sure the row was actually written.
    let refetched = fetch_job(&pool, third).await.unwrap().unwrap();
    assert_eq!(refetched.notes.as_deref(), Some("Heavier than declared"));

    // Step 8 — truck rollup snapshot BEFORE we delete anything. Manual jobs
    // don't create driving_session rows, so distance columns are all 0, but
    // total_jobs should reflect the four Scanias and three Volvos.
    let trucks_before = fetch_trucks(&pool).await.unwrap();
    assert_eq!(trucks_before.len(), 2);
    let scania_id = truck_id("Scania", "R 730");
    let volvo_id = truck_id("Volvo", "FH 540");
    let scania = trucks_before.iter().find(|t| t.id == scania_id).unwrap();
    let volvo = trucks_before.iter().find(|t| t.id == volvo_id).unwrap();
    assert_eq!(scania.total_jobs, 4);
    assert_eq!(volvo.total_jobs, 3);
    assert_eq!(scania.total_distance_km, 0.0);
    assert_eq!(volvo.total_distance_km, 0.0);

    // Step 9 — delete the patched (Scania) job. Truck rollup should drop the
    // count for Scania by 1, Volvo unchanged.
    remove_job(&pool, third).await.unwrap();
    let trucks_after = fetch_trucks(&pool).await.unwrap();
    let scania_after = trucks_after.iter().find(|t| t.id == scania_id).unwrap();
    let volvo_after = trucks_after.iter().find(|t| t.id == volvo_id).unwrap();
    assert_eq!(scania_after.total_jobs, 3, "Scania count drops by one");
    assert_eq!(volvo_after.total_jobs, 3, "Volvo count unchanged");
    // The deleted job is genuinely gone.
    assert!(fetch_job(&pool, third).await.unwrap().is_none());
    let final_listed = fetch_jobs(&pool, JobFilter::default()).await.unwrap();
    assert_eq!(final_listed.total, 6);
}
