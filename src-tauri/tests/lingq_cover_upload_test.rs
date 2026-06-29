use std::path::PathBuf;

use mockito::Matcher;
use secrecy::SecretString;

use lingq_upload_lib::lingq::client::LingqClient;
use lingq_upload_lib::lingq::collections::CollectionId;
use lingq_upload_lib::lingq::lang::LanguageCode;

fn ja() -> LanguageCode {
    LanguageCode::new("ja").expect("valid lang")
}

fn fixture_epub() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/epub-covers/epub3-properties.epub")
}

/// Combines both cascade scenarios in one test to avoid OnceLock cross-test
/// contamination (WINNER is process-global; parallel tests on different mockito
/// servers would race on the cached probe variant).
///
/// Scenario A: probe 1+2 return 404, probe 3 returns 200 → Ok(true) + caches winner.
/// Scenario B (winner already cached): second server's probe 3 returns 400 → Ok(false).
#[tokio::test]
async fn cover_upload_cascade_and_winner_cache() {
    // --- Scenario A: cascade finds probe 3, caches it ---
    let mut server_a = mockito::Server::new_async().await;
    let cid = CollectionId(42);
    let lang = "ja";

    let probe1 = server_a
        .mock("PATCH", format!("/api/v3/{lang}/collections/42/image/").as_str())
        .with_status(404)
        .expect(1)
        .create_async()
        .await;
    let probe2 = server_a
        .mock("PATCH", format!("/api/v3/{lang}/collections/42/").as_str())
        .match_body(Matcher::Regex(r#"name="image""#.into()))
        .with_status(404)
        .expect(1)
        .create_async()
        .await;
    let probe3 = server_a
        .mock("PATCH", format!("/api/v3/{lang}/collections/42/").as_str())
        .match_body(Matcher::Regex(r#"name="cover""#.into()))
        .with_status(200)
        .with_body("{}")
        // Called twice: initial cascade + second call hitting cached winner.
        .expect(2)
        .create_async()
        .await;

    let client_a = LingqClient::with_base_url(
        SecretString::new("token".into()),
        ja(),
        server_a.url(),
    );
    let img = fixture_epub();

    let ok = client_a.set_collection_image(cid, &img).await.unwrap();
    assert!(ok, "cascade should succeed on probe 3");

    // Second call on same client: winner cache skips probes 1 & 2.
    let ok2 = client_a.set_collection_image(cid, &img).await.unwrap();
    assert!(ok2, "cached winner should succeed immediately");

    probe1.assert_async().await;
    probe2.assert_async().await;
    probe3.assert_async().await;

    // --- Scenario B: winner is cached; new server's probe 3 returns 4xx → Ok(false) ---
    let mut server_b = mockito::Server::new_async().await;
    let _p = server_b
        .mock("PATCH", format!("/api/v3/{lang}/collections/9/").as_str())
        .with_status(400)
        .create_async()
        .await;

    let client_b = LingqClient::with_base_url(
        SecretString::new("token".into()),
        ja(),
        server_b.url(),
    );
    let result = client_b
        .set_collection_image(CollectionId(9), &img)
        .await
        .unwrap();
    assert!(!result, "cached winner returning 4xx → Ok(false)");
}
