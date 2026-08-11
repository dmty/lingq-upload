#![allow(dead_code)]

pub mod mk_fixture;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use futures::future::BoxFuture;
use lingq_upload_lib::commands::jobs::JobCancelMap;
use lingq_upload_lib::commands::transcribe::{
    detect_start_offset_impl, detection_availability_impl, detection_provider_factory,
    DetectionAvailability,
};
use lingq_upload_lib::core::identity::ProjectId;
use lingq_upload_lib::core::project::Project;
use lingq_upload_lib::core::store::{InMemoryProjectStore, JsonProjectStore, ProjectStore};
use lingq_upload_lib::error::AppError;
use lingq_upload_lib::events::DetectionPhase;
use lingq_upload_lib::ingest::{AudioSource, TextSource};
use lingq_upload_lib::transcribe::{
    DetectStartResult, DetectionSink, TranscribeConsent, TranscribeError, TranscribeErrorKind,
    TranscribeOpts, TranscribeProviderId, Transcriber, Transcript,
};
use secrecy::SecretString;
use tempfile::TempDir;
use uuid::Uuid;

const CHAPTER_BODIES: [&str; 8] = [
    "Blue lanterns marked the footpath while the observatory prepared for dawn.",
    "A patient clockmaker catalogued brass springs beside the rain-streaked window.",
    "At sunrise the copper weather vanes turned east above the quiet orchard wall.",
    "Three field notebooks described moss patterns found beyond the old stone bridge.",
    "By noon the kite makers tested violet sails against a steady coastal breeze.",
    "After dusk a silver tram carried the last gardeners home through falling snow.",
    "The bakery cooled rye loaves on cedar racks before the market bells sounded.",
    "Under a clear moon the survey team folded maps and stored every compass safely.",
];

#[derive(Clone, Copy, Debug)]
pub enum StoreKind {
    Memory,
    Json,
}

#[derive(Clone)]
pub struct CountingFakeTranscriber {
    calls: Arc<AtomicUsize>,
    paths: Arc<Mutex<Vec<PathBuf>>>,
    responses: Arc<Vec<String>>,
}

impl CountingFakeTranscriber {
    fn new() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            paths: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(vec![CHAPTER_BODIES[2].into(), CHAPTER_BODIES[5].into()]),
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn samples_are_cleaned(&self) -> bool {
        let paths = self.paths.lock().unwrap();
        !paths.is_empty() && paths.iter().all(|path| !path.exists())
    }
}

impl Transcriber for CountingFakeTranscriber {
    fn provider_id(&self) -> TranscribeProviderId {
        TranscribeProviderId::Groq
    }

    fn transcribe(
        &self,
        audio: &Path,
        _: &TranscribeOpts,
    ) -> BoxFuture<'_, Result<Transcript, TranscribeError>> {
        self.paths.lock().unwrap().push(audio.to_owned());
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self
            .responses
            .get(call)
            .cloned()
            .map(|text| Transcript { text })
            .ok_or_else(|| TranscribeError {
                kind: TranscribeErrorKind::ProviderFailed,
                message: "unexpected extra fake transcription call".into(),
            });
        Box::pin(async move { result })
    }
}

// ponytail: no-op sink — Task 15 asserts provider/store contracts, not event logs
#[derive(Clone, Default)]
pub struct RecordingDetectionSink;

impl DetectionSink for RecordingDetectionSink {
    fn started(&mut self, _: Uuid) {}
    fn progress(&mut self, _: Uuid, _: f32, _: DetectionPhase) {}
    fn result(&mut self, _: Uuid, _: &DetectStartResult) {}
    fn error(&mut self, _: Uuid, _: &TranscribeError) {}
    fn cancelled(&mut self, _: Uuid) {}
}

pub struct BackendFixture {
    pub store: Arc<dyn ProjectStore>,
    pub id: ProjectId,
    pub transcriber: CountingFakeTranscriber,
    pub sink: RecordingDetectionSink,
    store_kind: StoreKind,
    store_root: PathBuf,
    preferences_dir: TempDir,
    _source_dir: TempDir,
    _store_dir: TempDir,
}

impl BackendFixture {
    fn project(&self) -> Result<Project, AppError> {
        self.store
            .get(&self.id)
            .map_err(|error| AppError::Other(format!("store.get: {error}")))?
            .ok_or_else(|| AppError::Other("project not found".into()))
    }

    pub async fn availability(&self) -> Result<DetectionAvailability, AppError> {
        detection_availability_impl(&self.project()?, TranscribeProviderId::Groq, true).await
    }

    pub async fn detect(&self) -> Result<DetectStartResult, AppError> {
        let project = self.project()?;
        let transcriber = self.transcriber.clone();
        let factory = detection_provider_factory(
            self.store.as_ref(),
            &self.id,
            self.preferences_dir.path(),
            |_| Ok(SecretString::from("fixture-provider-key")),
            move |provider_id, _| {
                if provider_id != TranscribeProviderId::Groq {
                    return Err(TranscribeError {
                        kind: TranscribeErrorKind::Unauthorized,
                        message: "fixture consent must select Groq".into(),
                    });
                }
                Ok(Box::new(transcriber))
            },
        );
        let cancels: JobCancelMap = Arc::new(Mutex::new(HashMap::new()));
        let mut sink = self.sink.clone();
        detect_start_offset_impl(&project, &cancels, Uuid::new_v4(), &mut sink, factory).await
    }

    pub fn provider_calls(&self) -> usize {
        self.transcriber.calls()
    }

    pub fn detection_samples_are_cleaned(&self) -> bool {
        self.transcriber.samples_are_cleaned()
    }

    pub fn rename_chapter_source(&self, order: usize) -> std::io::Result<()> {
        let project = self.store.get(&self.id).unwrap().unwrap();
        let TextSource::LooseFiles { paths } = project.sources.text else {
            unreachable!("backend fixture uses loose text files");
        };
        let source = &paths[order];
        fs::rename(
            source,
            source.with_file_name(format!("renamed-chapter-{order:03}.txt")),
        )
    }

    pub fn reopened_store(&self) -> Arc<dyn ProjectStore> {
        match self.store_kind {
            StoreKind::Memory => Arc::clone(&self.store),
            StoreKind::Json => Arc::new(JsonProjectStore::new(&self.store_root)),
        }
    }
}

pub async fn backend_fixture(store_kind: StoreKind) -> BackendFixture {
    let source_dir = tempfile::tempdir().unwrap();
    let chapter_dir = source_dir.path().join("chapters");
    let audio_dir = source_dir.path().join("audio");
    fs::create_dir_all(&chapter_dir).unwrap();
    fs::create_dir_all(&audio_dir).unwrap();

    let text_paths = CHAPTER_BODIES
        .iter()
        .enumerate()
        .map(|(order, body)| {
            let path = chapter_dir.join(format!("chapter-{order:03}.txt"));
            fs::write(&path, body).unwrap();
            path
        })
        .collect();
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audio/probe_3min.mp3"),
        audio_dir.join("track-001.mp3"),
    )
    .unwrap();

    let mut project = Project::new_test(
        ProjectId::from_title_author("Backend detection", "Fixture Author"),
        "Backend detection",
    );
    project.sources.text = TextSource::LooseFiles { paths: text_paths };
    project.sources.audio = Some(AudioSource::Folder(audio_dir));
    project.transcribe_consent = Some(TranscribeConsent {
        provider_id: TranscribeProviderId::Groq,
        accepted_at: Utc::now(),
    });

    let store_dir = tempfile::tempdir().unwrap();
    let store_root = store_dir.path().to_path_buf();
    let store: Arc<dyn ProjectStore> = match store_kind {
        StoreKind::Memory => Arc::new(InMemoryProjectStore::new()),
        StoreKind::Json => Arc::new(JsonProjectStore::new(&store_root)),
    };
    store.put(&project).unwrap();

    let preferences_dir = tempfile::tempdir().unwrap();
    fs::write(
        preferences_dir
            .path()
            .join("transcription-preferences.json"),
        serde_json::to_vec(&serde_json::json!({
            "provider_id": TranscribeProviderId::Groq,
            "auto_detect_start": true,
        }))
        .unwrap(),
    )
    .unwrap();

    BackendFixture {
        store,
        id: project.id,
        transcriber: CountingFakeTranscriber::new(),
        sink: RecordingDetectionSink::default(),
        store_kind,
        store_root,
        preferences_dir,
        _source_dir: source_dir,
        _store_dir: store_dir,
    }
}
