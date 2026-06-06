mod commands;
mod core;
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
            commands::upload::upload_one_shot,
        ])
        // JobEvent isn't a command return; export it explicitly so the frontend
        // can type-narrow the raw "job" event payload.
        .typ::<events::JobEvent>()
        .typ::<events::Stage>()
        .typ::<events::LogLevel>()
        .typ::<lingq::LingqError>()
        .typ::<lingq::WhoAmI>()
        .typ::<lingq::LessonOpts>()
        .typ::<ingest::Candidate>()
        .typ::<ingest::TextSource>()
        .typ::<ingest::AudioSource>()
        .typ::<ingest::ChapterManifest>()
        .typ::<ingest::ChapterEntry>()
        .typ::<ingest::SeriesRef>()
        .typ::<ingest::IngestError>()
        .typ::<commands::upload::UploadResult>()
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
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
