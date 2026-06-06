# Component diagram

> Answers: *"Which Rust modules talk to which Svelte stores via which IPC commands?"* Replaces prose in AD-001 through AD-006.

```mermaid
flowchart LR
  subgraph SvelteKit["SvelteKit (SPA, Svelte 5 runes)"]
    UI_Library["/library page"]
    UI_Wizard["/new-project wizard"]
    UI_Settings["/settings page"]
    UI_Run["/project/[id] run view"]
    UI_OneShot["/ one-shot upload"]
    Store_Job["job.svelte.ts<br/>(JobEvent stream)"]
    Store_Projects["projects.svelte.ts"]
    Store_Settings["settings.svelte.ts"]
    Bindings["lib/ipc/bindings.ts<br/>(specta-generated)"]
  end

  subgraph Tauri["Tauri 2 host"]
    Cmd_Project["commands/project.rs"]
    Cmd_Parse["commands/parse.rs"]
    Cmd_Mapping["commands/mapping.rs"]
    Cmd_Carve["commands/carve.rs"]
    Cmd_Transcode["commands/transcode.rs"]
    Cmd_Upload["commands/upload.rs"]
    Cmd_Verify["commands/verify.rs"]
    Cmd_Secrets["commands/secrets.rs"]
    Events["events.rs<br/>emit('job', JobEvent)"]
    ErrorEnum["error.rs<br/>AppError"]
  end

  subgraph Core["Rust core (pure)"]
    Epub["core/epub + strategies"]
    Ruby["core/ruby"]
    Matcher["core/matcher"]
    Carver["core/carver"]
    Audio["core/audio"]
    Lingq["core/lingq client"]
    Project["core/project state"]
  end

  subgraph Pluggable["Strategy registries (public extension surface)"]
    Codecs["codecs/ AudioCodec"]
    Languages["languages/ LanguageProfile"]
    Headings["epub/strategies/ HeadingStrategy"]
  end

  subgraph External["External"]
    Keychain[(OS keychain)]
    Ffmpeg[(bundled ffmpeg)]
    LingqAPI[(LingQ API v3)]
    Disk[(project.json)]
  end

  UI_Library --> Bindings
  UI_Wizard --> Bindings
  UI_Settings --> Bindings
  UI_Run --> Bindings
  UI_OneShot --> Bindings

  Bindings -->|invoke| Cmd_Project
  Bindings -->|invoke| Cmd_Parse
  Bindings -->|invoke| Cmd_Mapping
  Bindings -->|invoke| Cmd_Carve
  Bindings -->|invoke| Cmd_Transcode
  Bindings -->|invoke| Cmd_Upload
  Bindings -->|invoke| Cmd_Verify
  Bindings -->|invoke| Cmd_Secrets

  Events -.->|listen 'job'| Store_Job
  Store_Job -.-> UI_Run
  Store_Job -.-> UI_OneShot

  Cmd_Parse --> Epub
  Cmd_Mapping --> Matcher
  Cmd_Carve --> Carver
  Cmd_Carve --> Ruby
  Cmd_Transcode --> Audio
  Cmd_Upload --> Lingq
  Cmd_Verify --> Audio
  Cmd_Verify --> Lingq
  Cmd_Project --> Project
  Cmd_Secrets --> External
  Cmd_Secrets -. uses .-> Keychain

  Epub --> Headings
  Audio --> Codecs
  Carver --> Languages
  Lingq --> Languages

  Audio --> Ffmpeg
  Lingq --> LingqAPI
  Project --> Disk

  Cmd_Parse -.->|emit| Events
  Cmd_Transcode -.->|emit| Events
  Cmd_Upload -.->|emit| Events
  Cmd_Verify -.->|emit| Events

  Cmd_Project -.->|Result| ErrorEnum
  Cmd_Upload -.->|Result| ErrorEnum
```

Solid arrows = synchronous `invoke()` request/response. Dashed arrows = event streams (`tauri::Window::emit` / `listen`).

## Read-it-in-30-seconds

- Frontend only talks to backend through `bindings.ts` (specta-generated, never hand-edited).
- Every `#[tauri::command]` returns `Result<T, AppError>`. No `anyhow` at the boundary.
- Every long-running command emits `JobEvent`s on the `"job"` channel; frontend stores subscribe.
- `core/*` is pure Rust — no Tauri imports. Swap-able for a CLI driver if ever needed.
- The three strategy registries (`codecs/`, `languages/`, `epub/strategies/`) are the public extension surface (AD-018).
