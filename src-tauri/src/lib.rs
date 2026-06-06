mod commands;
mod error;
mod events;

use specta_typescript::Typescript;
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
        ])
        // JobEvent isn't a command return; export it explicitly so the frontend
        // can type-narrow the raw "job" event payload.
        .typ::<events::JobEvent>()
        .typ::<events::Stage>()
        .typ::<events::LogLevel>()
}

/// Write the TypeScript bindings to `src/lib/ipc/bindings.ts`.
pub fn export_bindings() -> Result<(), Box<dyn std::error::Error>> {
    specta_builder().export(
        Typescript::default().header(
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
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
