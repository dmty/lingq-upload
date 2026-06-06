//! Live LingQ smoke. Runs only with `--ignored` and `LINGQ_API_KEY` env var set.
//! Documented in `docs/qa/sprint-01-smoke.md`.
//!
//! Run: `cargo test --manifest-path src-tauri/Cargo.toml --test manual_lingq_smoke -- --ignored`

use lingq_upload_lib::lingq::LingqClient;
use secrecy::SecretString;

#[tokio::test]
#[ignore]
async fn live_whoami() {
    let key = std::env::var("LINGQ_API_KEY").expect("LINGQ_API_KEY required");
    let lang = std::env::var("LINGQ_LANG").unwrap_or_else(|_| "ja".into());

    let client = LingqClient::new(SecretString::from(key), lang);
    let res = client.whoami().await.expect("live whoami");
    assert!(res.ok);
}
