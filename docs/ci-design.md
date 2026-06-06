# CI / CD design

Single GitHub Actions workflow `.github/workflows/ci.yml` runs on `push` to `main` and on every pull request.

## Goals

- Catch portability bugs early — every test runs on macOS, Windows, Ubuntu.
- Enforce the no-`unwrap()` / no-clippy-warnings discipline.
- Stay fast — under ~12 min on a warm cache, ~25 min cold.
- Never hit the live LingQ API. Cassettes + `mockito` only.

## Jobs

### `test` (matrix)

Runs on macOS, Windows, Ubuntu. Each entry does:

1. Install ffmpeg (brew / apt / choco depending on OS).
2. Install Linux-only system deps for Tauri 2 (webkit2gtk, etc.).
3. Set up stable Rust + clippy.
4. Set up Bun.
5. `Swatinem/rust-cache` for the `src-tauri` workspace.
6. `bun install --frozen-lockfile`.
7. `bun run check` — SvelteKit type check.
8. `bun run build` — frontend production build smoke.
9. `cargo build --locked` in `src-tauri`.
10. `cargo test --locked` in `src-tauri`.
11. `cargo clippy --locked -- -D warnings`.
12. `bun tauri build --debug` — Tauri smoke build.

`#[ignore]`-marked tests (cassette recorders, live-API smokes) are NOT run.

### `coverage` (Ubuntu only)

`cargo-llvm-cov` produces `lcov.info`, uploaded as a workflow artefact. Coverage thresholds (≥ 80% line coverage on `secrets`, `lingq`, `audio`) enforced via a follow-up shell step that greps the lcov output. No third-party coverage host required.

### `deny` (Ubuntu only)

`cargo-deny check` — licence + advisory check.

### `unwrap-grep` (Ubuntu only)

Greps `src-tauri/src` for `unwrap()` outside `#[cfg(test)]` modules. Single bash step; fails the build on any hit.

## Specta codegen handling

`bindings.ts` is checked in but is **generated** by `cargo build` via `build.rs`. CI verifies no diff after build:

```sh
git diff --exit-code src/lib/ipc/bindings.ts || \
  (echo "bindings.ts drifted from rust signatures — rebuild and commit" && exit 1)
```

Step lives in the `test` matrix's Ubuntu run.

## Live API & nightly canary

A nightly canary (re-records LingQ cassettes against the live API, diffs against the checked-in fixtures, opens an issue on shape drift) is planned as a separate workflow `lingq-canary.yml`. Not yet implemented.

## Caching strategy

- **Rust** — `Swatinem/rust-cache@v2` keyed on `Cargo.lock`. ~70% hit ratio across PRs in practice.
- **Bun** — `~/.bun/install/cache` keyed on `bun.lock`.
- **Tauri WebKit deps** — APT cache via `awalsh128/cache-apt-pkgs-action` on Ubuntu.

## Concurrency

`concurrency: { group: ${{ github.workflow }}-${{ github.ref }}, cancel-in-progress: true }` — superseded PR pushes cancel in-flight runs.

## Future work

- **`release.yml`** — tag-triggered, builds signed installers.
- **`lingq-canary.yml`** — nightly schema-drift detector.
- **Playwright E2E job** — full-stack golden-path against `bun tauri dev`.

## Why not GitHub Actions matrix include / exclude trickery

Initial pass uses one flat matrix. When the job graph grows past ~6 entries, split into a reusable workflow. Don't optimise the YAML until it hurts.
