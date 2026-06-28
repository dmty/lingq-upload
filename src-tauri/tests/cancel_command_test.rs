//! Black-box tests for `cmd_project_cancel`.
//!
//! Drives the Tauri-free `cancel_project_impl` helper directly so the test can
//! observe token state without spinning up a tauri::State plumbing dance.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use lingq_upload_lib::commands::jobs::{cancel_project_impl, JobCancelMap};
use lingq_upload_lib::core::identity::ProjectId;

fn empty_map() -> JobCancelMap {
    Arc::new(Mutex::new(HashMap::new()))
}

fn insert(map: &JobCancelMap, project: &ProjectId) -> CancellationToken {
    let tok = CancellationToken::new();
    map.lock()
        .unwrap()
        .insert(Uuid::new_v4(), (project.clone(), tok.clone()));
    tok
}

#[tokio::test]
async fn project_cancel_fires_matching_tokens_within_500ms() {
    let map = empty_map();
    let proj_a = ProjectId::from_title_author("Book A", "Author");
    let proj_b = ProjectId::from_title_author("Book B", "Author");

    let tok_a1 = insert(&map, &proj_a);
    let tok_a2 = insert(&map, &proj_a);
    let tok_b = insert(&map, &proj_b);

    let start = Instant::now();
    let fired = cancel_project_impl(&map, &proj_a);
    let elapsed = start.elapsed();

    assert_eq!(fired, 2, "fired count should match A entries");
    assert!(
        elapsed < Duration::from_millis(500),
        "contract: cancel returns within 500ms, took {elapsed:?}",
    );

    // Wall-clock check that the matching tokens have observably fired.
    let wait_a1 = tokio::time::timeout(Duration::from_millis(500), tok_a1.cancelled()).await;
    let wait_a2 = tokio::time::timeout(Duration::from_millis(500), tok_a2.cancelled()).await;
    assert!(wait_a1.is_ok(), "tok_a1 should be cancelled");
    assert!(wait_a2.is_ok(), "tok_a2 should be cancelled");
    assert!(
        !tok_b.is_cancelled(),
        "tok_b for unrelated project must not fire"
    );
}

#[tokio::test]
async fn project_cancel_with_no_matching_jobs_is_noop() {
    let map = empty_map();
    let proj = ProjectId::from_title_author("Nobody Home", "Author");

    // Empty map.
    let fired = cancel_project_impl(&map, &proj);
    assert_eq!(fired, 0);

    // Populated map, different project.
    let other = ProjectId::from_title_author("Different Book", "Author");
    let tok_other = insert(&map, &other);

    let fired = cancel_project_impl(&map, &proj);
    assert_eq!(fired, 0);
    assert!(!tok_other.is_cancelled());
}
