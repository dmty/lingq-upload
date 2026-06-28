# Project state machine

> Answers: *"What can the UI show for this project right now, and which transitions are legal?"* Drives error model and resumability per AD-007 and AD-010.

```mermaid
stateDiagram-v2
  [*] --> New: cmd_create_project

  New --> Parsing: cmd_parse_epub
  Parsing --> Parsed: ok
  Parsing --> New: err (autosaved with last_error)

  Parsed --> Matching: cmd_auto_match
  Matching --> Mapped: ok
  Matching --> Parsed: err

  Mapped --> Mapped: user_edits_mapping (auto-save)
  Mapped --> Carving: cmd_carve
  Carving --> Carved: ok
  Carving --> Mapped: err

  Carved --> Transcoding: cmd_transcode_all
  Transcoding --> Transcoding: per-track progress (sequential)
  Transcoding --> Transcoded: ok
  Transcoding --> Carved: err (per-track skipped, transcript persisted)

  Transcoded --> Uploading: cmd_upload_all
  Uploading --> Uploading: per-lesson progress
  Uploading --> Uploaded: all lessons accepted
  Uploading --> Transcoded: err (resumable — uploaded lessons persisted)

  Uploaded --> Verifying: cmd_verify_integrity (auto on Uploaded)
  Verifying --> Done: all durations match
  Verifying --> NeedsFix: deltas > 1.0s on N tracks

  NeedsFix --> FixingCorrupted: cmd_replace_audio_for_corrupted
  FixingCorrupted --> Verifying: ok
  FixingCorrupted --> NeedsFix: err

  Done --> [*]

  state "Any" as Any {
    direction LR
    [*] --> Cancelled: cmd_cancel_job(job_id)
    Cancelled --> [*]
  }
```

## Stage definitions

| Stage | Persisted invariant |
|---|---|
| `New` | sources picked, settings defaulted, no derived data |
| `Parsed` | EPUB headings extracted, `epub.headings[]` populated |
| `Mapped` | `mapping[]` complete, every track has a `headingId` or is explicitly unmapped |
| `Carved` | per-track text materialised, byte-stable |
| `Transcoded` | every track has a verified mp3 with `mp3Sec` within 1.0s of source |
| `Uploaded` | every track has `lingq.lessons[trackId].lessonId` |
| `Verifying` | integrity check in progress (transient) |
| `NeedsFix` | one or more tracks failed integrity vs LingQ-side audio duration |
| `Done` | uploaded + verified + no outstanding issues |

## Resumability contract

- **Every stage transition is atomic.** `project.json` is rewritten via tempfile + rename. A crash mid-stage means the previous stage's snapshot is intact.
- **Stages are idempotent.** Re-running `cmd_transcode_all` on a project already in `Transcoded` is a no-op (per-track `mp3Sec` already present).
- **Per-track granularity inside Transcoding / Uploading.** The job persists each completed track before moving on. Resume after a kill picks up at the next unfinished track.
- **Cancel** is observed as a global event but lands the project in the previous *stable* stage with a partial-progress record (`integrity.byTrack[i]` set for done tracks, absent for the rest).
