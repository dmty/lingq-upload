pub mod commands;
pub mod core;
mod error;
pub mod events;
pub mod ingest;
pub mod lingq;
mod secrets;

use specta_typescript::{BigIntExportBehavior, Typescript};
use tauri_specta::{collect_commands, Builder};

/// Build the tauri-specta Builder with all registered commands and exported types.
///
/// Used both by the running app (`run()`) and by the bindings exporter
/// (`tests/bindings.rs` / `bin/gen_bindings.rs`) so the two cannot drift.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::ping::ping,
            commands::demo::start_demo_job,
            commands::secrets::cmd_save_lingq_key,
            commands::secrets::cmd_load_lingq_key,
            commands::secrets::cmd_clear_lingq_key,
            commands::secrets::cmd_get_dev_backend,
            commands::secrets::cmd_set_dev_backend,
            commands::ingest::manual_source_from_files,
            commands::ingest::cmd_ingest_scan,
            commands::files::cmd_expand_audio_dir,
            commands::lingq::cmd_account_profile,
            commands::lingq::cmd_list_languages,
            commands::lingq::cmd_list_collections,
            commands::upload::upload_one_shot,
            commands::library::cmd_library_list,
            commands::library::cmd_trash_project,
            commands::library::cmd_list_trash,
            commands::library::cmd_restore_project,
            commands::library::cmd_purge_project,
            commands::add_project::cmd_create_project,
            commands::add_project::cmd_create_project_with_resolution,
            commands::matcher::cmd_matcher_resolve,
            commands::matcher::cmd_matcher_inspect,
            commands::matcher::cmd_replace_audio_source,
            commands::matcher::cmd_seed_mapping,
            commands::mapping::cmd_apply_mapping_op,
            commands::project::cmd_project_load,
            commands::project::cmd_project_chapters,
            commands::project::cmd_chapter_text,
            commands::project::cmd_set_selection,
            commands::project::cmd_set_absorb_policy,
            commands::project::cmd_set_cover,
            commands::project::cmd_confirm_mapping,
            commands::jobs::cmd_start_project_job,
            commands::jobs::cmd_cancel_job,
            commands::jobs::cmd_project_cancel,
            commands::jobs::cmd_replay_receipts,
        ])
        // JobEvent isn't a command return; export it explicitly so the frontend
        // can type-narrow the raw "job" event payload.
        .typ::<events::JobEvent>()
        .typ::<events::Stage>()
        .typ::<events::LogLevel>()
        .typ::<lingq::LingqError>()
        .typ::<lingq::WhoAmI>()
        .typ::<lingq::LessonOpts>()
        .typ::<lingq::Language>()
        .typ::<lingq::Collection>()
        .typ::<lingq::AccountProfile>()
        .typ::<ingest::Candidate>()
        .typ::<ingest::TextSource>()
        .typ::<ingest::AudioSource>()
        .typ::<ingest::ChapterManifest>()
        .typ::<ingest::ChapterEntry>()
        .typ::<ingest::SeriesRef>()
        .typ::<ingest::IngestError>()
        .typ::<commands::upload::UploadResult>()
        .typ::<commands::secrets::DevBackendInfo>()
        .typ::<secrets::BackendChoice>()
        .typ::<core::identity::ProjectId>()
        .typ::<core::epub::Chapter>()
        .typ::<core::epub::ChapterKind>()
        .typ::<commands::project::ChapterMeta>()
        .typ::<core::audio::AbsorbPolicy>()
        .typ::<core::project::Project>()
        .typ::<core::project::ProjectSummary>()
        .typ::<core::project::ChapterReceipt>()
        .typ::<core::project::MatcherDecision>()
        .typ::<core::matcher::MismatchCondition>()
        .typ::<core::matcher::MismatchResponse>()
        .typ::<core::matcher::BucketPreview>()
        .typ::<core::matcher::MappingOp>()
        .typ::<core::matcher::MappingPair>()
        .typ::<core::matcher::MappingState>()
        .typ::<core::matcher::MappingError>()
        .typ::<core::job::MismatchInspection>()
        .typ::<core::library::LibraryIndex>()
        .typ::<core::library::LibraryEntry>()
        .typ::<core::library::LibraryStatus>()
        .typ::<core::library::TrashEntry>()
        .typ::<commands::add_project::CreateProjectResult>()
        .typ::<commands::add_project::ConflictResolution>()
        .typ::<commands::ingest::LibrarySource>()
        .typ::<commands::jobs::ReceiptSnapshot>()
}

/// Write the TypeScript bindings to `src/lib/ipc/bindings.ts`.
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    // LingQ collection / lesson IDs fit in JS Number range; map i64 -> number.
    specta_builder().export(
        Typescript::default()
            .bigint(BigIntExportBehavior::Number)
            .header(
                "// @ts-nocheck\n// AUTO-GENERATED. Do not edit. specta from #[tauri::command]\n",
            ),
        "../src/lib/ipc/bindings.ts",
    )?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let builder = specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            use std::collections::HashMap;
            use std::sync::{Arc, Mutex};
            use tauri::Manager;
            register_bundled_audio_binaries(app);
            let root = app
                .path()
                .app_data_dir()
                .expect("app_data_dir resolves at startup");
            let store: Arc<dyn core::store::ProjectStore> =
                Arc::new(core::store::JsonProjectStore::new(root));
            app.manage(store);
            let cancels: commands::jobs::JobCancelMap = Arc::new(Mutex::new(HashMap::new()));
            app.manage(cancels);
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Resolve the bundled ffmpeg / ffprobe under `resource_dir/ffmpeg/<os>/` and
/// hand them to the audio module. Layout matches `docs/dev-setup.md`.
fn register_bundled_audio_binaries(app: &tauri::App) {
    use tauri::Manager;

    let Ok(resource_dir) = app.path().resource_dir() else {
        return;
    };
    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };
    let (ffmpeg_name, ffprobe_name) = if cfg!(target_os = "windows") {
        ("ffmpeg.exe", "ffprobe.exe")
    } else {
        ("ffmpeg", "ffprobe")
    };
    let base = resource_dir.join("ffmpeg").join(platform);
    core::audio::set_bundled_binaries(base.join(ffmpeg_name), base.join(ffprobe_name));
}
