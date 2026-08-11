mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lingq_upload_lib::commands::jobs::JobCancelMap;
use lingq_upload_lib::commands::transcribe::{
    confirm_detected_range_impl, detect_start_offset_impl, detection_availability_impl,
};
use lingq_upload_lib::core::epub::ChapterId;
use lingq_upload_lib::core::matcher::{MappingPair, MismatchResponse};
use lingq_upload_lib::transcribe::{
    DetectStartResult, DetectedRange, DetectionPreview, TranscribeProviderId,
};
use uuid::Uuid;

use support::{backend_fixture, BackendFixture, StoreKind};

fn cid(order: usize) -> ChapterId {
    ChapterId::from_order(order)
}

fn detected_preview(result: DetectStartResult) -> DetectionPreview {
    let DetectStartResult::Detected { preview } = result else {
        panic!("expected a detected range, got {result:?}");
    };
    preview
}

fn mapped_chapter_ids(pairs: &[MappingPair]) -> Vec<ChapterId> {
    pairs.iter().map(|pair| pair.chapter_id.clone()).collect()
}

async fn detect_and_confirm(fixture: &BackendFixture) -> DetectionPreview {
    let availability = fixture.availability().await.unwrap();
    assert!(availability.eligible);
    assert!(availability.key_present);
    assert!(availability.consent_matches);
    assert!(availability.existing_evidence.is_none());
    assert!(availability.can_start);

    let preview = detected_preview(fixture.detect().await.unwrap());
    assert_eq!(
        preview.range,
        DetectedRange {
            start_chapter_id: cid(2),
            end_chapter_id: cid(5),
        }
    );
    assert_eq!(fixture.provider_calls(), 2);
    assert!(fixture.detection_samples_are_cleaned());

    confirm_detected_range_impl(
        fixture.store.as_ref(),
        &fixture.id,
        preview.range.clone(),
        preview.clone(),
    )
    .await
    .unwrap();
    preview
}

#[tokio::test]
async fn fake_provider_to_persisted_bounded_mapping_round_trips_on_each_store() {
    for store_kind in [StoreKind::Memory, StoreKind::Json] {
        let fixture = backend_fixture(store_kind).await;
        let preview = detect_and_confirm(&fixture).await;
        let reloaded = fixture.store.get(&fixture.id).unwrap().unwrap();
        let decision = reloaded.matcher_decision.unwrap();

        assert_eq!(decision.response, MismatchResponse::SplitProportional);
        assert_eq!(decision.detection.unwrap().range, preview.range);
        assert_eq!(
            mapped_chapter_ids(&reloaded.mapping.unwrap().pairs),
            vec![cid(2), cid(3), cid(4), cid(5)]
        );
    }
}

#[tokio::test]
async fn renamed_loose_source_rejects_stale_confirmation_without_mutation() {
    let fixture = backend_fixture(StoreKind::Json).await;
    let preview = detected_preview(fixture.detect().await.unwrap());
    fixture.rename_chapter_source(3).unwrap();
    let before = fixture.store.get(&fixture.id).unwrap().unwrap();

    let error = confirm_detected_range_impl(
        fixture.store.as_ref(),
        &fixture.id,
        preview.range.clone(),
        preview,
    )
    .await
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("text source changed; rerun or refine detection"),
        "unexpected stale-source error: {error}"
    );
    assert_eq!(fixture.store.get(&fixture.id).unwrap().unwrap(), before);
    assert_eq!(fixture.provider_calls(), 2);
}

#[tokio::test]
async fn reloaded_evidence_is_reused_without_another_transcriber_call() {
    let fixture = backend_fixture(StoreKind::Json).await;
    let preview = detect_and_confirm(&fixture).await;
    let reopened = fixture.reopened_store();
    let reloaded = reopened.get(&fixture.id).unwrap().unwrap();

    let availability = detection_availability_impl(&reloaded, TranscribeProviderId::Groq, true)
        .await
        .unwrap();
    assert!(!availability.can_start);
    assert_eq!(availability.existing_evidence.unwrap().range, preview.range);

    let cancels: JobCancelMap = Arc::new(Mutex::new(HashMap::new()));
    let mut sink = fixture.sink.clone();
    let transcriber = fixture.transcriber.clone();
    let result = detect_start_offset_impl(
        &reloaded,
        &cancels,
        Uuid::new_v4(),
        &mut sink,
        Box::new(move || Ok(Box::new(transcriber))),
    )
    .await
    .unwrap();

    assert_eq!(detected_preview(result).range, preview.range);
    assert_eq!(fixture.provider_calls(), 2);
}
