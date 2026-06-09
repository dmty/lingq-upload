use lingq_upload_lib::core::audio::AbsorbPolicy;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::matcher::{MismatchCondition, MismatchResponse};
use lingq_upload_lib::core::project::{
    ChapterReceipt, MatcherDecision, Project, ProjectSettings, ProjectSources, SCHEMA_V1,
};
use lingq_upload_lib::core::queue::Queue;
use lingq_upload_lib::core::store::{InMemoryProjectStore, ProjectStore};
use lingq_upload_lib::ingest::TextSource;

fn project(title: &str, receipts: Vec<ChapterReceipt>) -> Project {
    Project {
        schema_version: SCHEMA_V1,
        id: ProjectId::from_title_author(title, "Author"),
        sources: ProjectSources {
            text: TextSource::Epub(PathBuf::from("/tmp/x.epub")),
            audio: None,
            chapter_manifest: None,
        },
        settings: ProjectSettings {
            language: "en".into(),
            collection_title: title.into(),
            level: 1,
            tags: vec![],
        },
        receipts,
        queue_cursor: 0,
        completed_lesson_ids: vec![],
        matcher_decision: None,
        cover_path: None,
        authors: vec![],
        series: None,
        lingq_collection_id: None,
        last_activity_at: None,
        stage: Default::default(),
        last_transition_at: None,
    skipped_chapters: vec![],
    absorb_policy: AbsorbPolicy::default(),
    }
}

fn done_receipt(i: usize) -> ChapterReceipt {
    ChapterReceipt {
        chapter_index: i,
        track_index: Some(i),
        lesson_id: Some(100 + i as i64),
        degraded: false,
        uploaded_at: Some(Utc::now()),
    }
}

fn pending_receipt(i: usize) -> ChapterReceipt {
    ChapterReceipt {
        chapter_index: i,
        track_index: Some(i),
        lesson_id: None,
        degraded: false,
        uploaded_at: None,
    }
}

#[test]
fn push_then_current_returns_pushed_job() {
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    let q = Queue::new(store);
    let id = ProjectId::from_title_author("Foo", "Bar");
    let job_id = q.push(id.clone());
    let cur = q.current().unwrap();
    assert_eq!(cur.id, job_id);
    assert_eq!(cur.project_id, id);
}

#[test]
fn advance_pops_front_in_order() {
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    let q = Queue::new(store);
    let a = q.push(ProjectId::from_title_author("A", "x"));
    let b = q.push(ProjectId::from_title_author("B", "x"));
    assert_eq!(q.advance(), Some(a));
    assert_eq!(q.advance(), Some(b));
    assert_eq!(q.advance(), None);
}

#[test]
fn exactly_one_job_is_current() {
    let store: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());
    let q = Queue::new(store);
    q.push(ProjectId::from_title_author("A", "x"));
    q.push(ProjectId::from_title_author("B", "x"));
    q.push(ProjectId::from_title_author("C", "x"));
    let _ = q.current();
    assert_eq!(q.len(), 3);
}

#[test]
fn rebuild_pending_re_enqueues_unfinished_projects() {
    let store_arc: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());

    let mut finished = project("Done", vec![done_receipt(0), done_receipt(1)]);
    finished.queue_cursor = 2; // fully advanced
    let pending = project("Pending", vec![done_receipt(0), pending_receipt(1)]);
    let no_receipts = project("Untouched", vec![]);

    store_arc.put(&finished).unwrap();
    store_arc.put(&pending).unwrap();
    store_arc.put(&no_receipts).unwrap();

    let q = Queue::new(Arc::clone(&store_arc));
    let added = q.rebuild_pending().unwrap();
    assert_eq!(added, 1, "only the half-done project re-enqueues");
    let cur = q.current().unwrap();
    assert_eq!(cur.project_id, pending.id);
}

#[test]
fn rebuild_pending_re_enqueues_decided_but_unstarted_projects() {
    let store_arc: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());

    let mut decided = project("Decided", vec![]);
    decided.matcher_decision = Some(MatcherDecision {
        condition: MismatchCondition::CountOff,
        response: MismatchResponse::PairAccept,
        chapter_count: 5,
        track_count: 6,
        user_overrode: false,
        decided_at: Utc::now(),
    });
    let untouched = project("Untouched", vec![]);

    store_arc.put(&decided).unwrap();
    store_arc.put(&untouched).unwrap();

    let q = Queue::new(Arc::clone(&store_arc));
    let added = q.rebuild_pending().unwrap();
    assert_eq!(
        added, 1,
        "only the decided-but-unstarted project re-enqueues"
    );
    let cur = q.current().unwrap();
    assert_eq!(cur.project_id, decided.id);
}

#[test]
fn rebuild_pending_re_enqueues_cursor_lag() {
    let store_arc: Arc<dyn ProjectStore> = Arc::new(InMemoryProjectStore::new());

    let mut lagging = project("Lagging", vec![done_receipt(0), done_receipt(1)]);
    lagging.queue_cursor = 1; // cursor hasn't advanced past second receipt

    store_arc.put(&lagging).unwrap();

    let q = Queue::new(Arc::clone(&store_arc));
    let added = q.rebuild_pending().unwrap();
    assert_eq!(
        added, 1,
        "cursor lag re-enqueues even when all receipts have lesson_ids"
    );
}
