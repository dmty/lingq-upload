mod commands;
pub mod core;
mod error;
mod events;
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
            commands::ingest::manual_source_from_files,
            commands::lingq::cmd_account_profile,
            commands::lingq::cmd_list_languages,
            commands::lingq::cmd_list_collections,
            commands::upload::upload_one_shot,
            commands::library::cmd_library_list,
            commands::add_project::cmd_create_project,
            commands::matcher::cmd_matcher_resolve,
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
        .typ::<core::identity::ProjectId>()
        .typ::<core::project::Project>()
        .typ::<core::project::ProjectSummary>()
        .typ::<core::project::ChapterReceipt>()
        .typ::<core::project::MatcherDecision>()
        .typ::<core::matcher::MismatchCondition>()
        .typ::<core::matcher::MismatchResponse>()
        .typ::<core::library::LibraryIndex>()
        .typ::<core::library::LibraryEntry>()
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
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let builder = specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            register_bundled_audio_binaries(app);
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
