use lingq_upload_lib::core::identity::{content_hash, IdentityError, ProjectId};
use uuid::Uuid;

#[test]
fn content_hash_stable_after_nfc_and_whitespace() {
    // NFD form of "海" composed differently produces same hash.
    let a = ProjectId::from_title_author(" 海辺のカフカ 下 ", "村上 春樹");
    let b = ProjectId::from_title_author("海辺のカフカ  下", " 村上  春樹 ");
    assert_eq!(a.content_hash, b.content_hash);
}

#[test]
fn content_hash_case_insensitive_latin() {
    let a = content_hash("Kafka On The Shore", "Haruki Murakami");
    let b = content_hash("kafka on the shore", "haruki murakami");
    assert_eq!(a, b);
}

#[test]
fn matches_strong_key_asin() {
    let mut a = ProjectId::from_title_author("Foo", "Bar");
    let mut b = ProjectId::from_title_author("Different", "Author");
    a = a.with_asin("B0XXXX1");
    b = b.with_asin("B0XXXX1");
    assert!(a.matches(&b));
}

#[test]
fn matches_strong_key_isbn() {
    let mut a = ProjectId::from_title_author("Foo", "Bar");
    let mut b = ProjectId::from_title_author("Different", "Author");
    a = a.with_isbn13("9784101001012");
    b = b.with_isbn13("9784101001012");
    assert!(a.matches(&b));
}

#[test]
fn matches_falls_back_to_content_hash() {
    let a = ProjectId::from_title_author("Same Title", "Same Author");
    let b = ProjectId::from_title_author("Same Title", "Same Author");
    assert!(a.matches(&b));
}

#[test]
fn no_match_when_strong_keys_disagree_and_hash_differs() {
    let a = ProjectId::from_title_author("Foo", "Bar").with_asin("B001");
    let b = ProjectId::from_title_author("Baz", "Qux").with_asin("B002");
    assert!(!a.matches(&b));
}

#[test]
fn none_slots_ignored_in_match() {
    let a = ProjectId::from_title_author("X", "Y").with_isbn13("9784101001012");
    let b = ProjectId::from_title_author("X", "Y"); // no isbn, but same hash
    assert!(a.matches(&b));
}

#[test]
fn join_key_precedence_asin_over_isbn() {
    let id = ProjectId::from_title_author("X", "Y")
        .with_asin("B0XXXX")
        .with_isbn13("9784101001012")
        .with_calibre_uuid(Uuid::nil());
    assert_eq!(id.join_key(), "asin:B0XXXX");
}

#[test]
fn join_key_precedence_isbn_over_uuid() {
    let id = ProjectId::from_title_author("X", "Y")
        .with_isbn13("9784101001012")
        .with_calibre_uuid(Uuid::nil());
    assert_eq!(id.join_key(), "isbn13:9784101001012");
}

#[test]
fn join_key_precedence_uuid_over_hash() {
    let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let id = ProjectId::from_title_author("X", "Y").with_calibre_uuid(uuid);
    assert_eq!(id.join_key(), format!("uuid:{uuid}"));
}

#[test]
fn join_key_fallback_to_content_hash() {
    let id = ProjectId::from_title_author("X", "Y");
    let key = id.join_key();
    assert!(key.starts_with("ch:"));
    assert_eq!(key.len(), 3 + 64);
}

#[test]
fn matches_same_asin_different_titles() {
    let a = ProjectId::from_title_author("Kafka on the Shore", "Murakami").with_asin("B0ABCDEFGH");
    let b = ProjectId::from_title_author("totally different", "different person")
        .with_asin("B0ABCDEFGH");
    assert!(a.matches(&b));
}

#[test]
fn matches_when_asin_agrees_even_if_isbn_differs() {
    // join_key resolution: asin wins; both sides share the same asin so they
    // still match, even though the isbn slots disagree.
    let a = ProjectId::from_title_author("X", "Y")
        .with_asin("B0ABCDEFGH")
        .with_isbn13("9780000000001");
    let b = ProjectId::from_title_author("Q", "R")
        .with_asin("B0ABCDEFGH")
        .with_isbn13("9780000000002");
    assert!(a.matches(&b));
}

#[test]
fn matches_only_one_strong_key_each_side_falls_back_to_hash() {
    // a has only asin; b has only isbn; titles differ → no match.
    let a = ProjectId::from_title_author("X", "Y").with_asin("B0AAAAAAAA");
    let b = ProjectId::from_title_author("Q", "R").with_isbn13("9780000000001");
    assert!(!a.matches(&b));

    // Same a, same title/author on b but with isbn → hash matches.
    let c = ProjectId::from_title_author("X", "Y").with_isbn13("9780000000001");
    assert!(a.matches(&c));
}

#[test]
fn with_isbn13_drops_invalid_silently() {
    let id = ProjectId::from_title_author("X", "Y").with_isbn13("not-an-isbn");
    assert!(id.isbn13.is_none());
}

#[test]
fn with_isbn13_strips_hyphens_and_whitespace() {
    let id = ProjectId::from_title_author("X", "Y").with_isbn13("978-4-10-100101-2");
    assert_eq!(id.isbn13.as_deref(), Some("9784101001012"));
}

#[test]
fn with_isbn13_rejects_x_check_digit() {
    // X is ISBN-10 only.
    let id = ProjectId::from_title_author("X", "Y").with_isbn13("978410100101X");
    assert!(id.isbn13.is_none());
}

#[test]
fn try_with_isbn13_returns_err_on_invalid() {
    let r = ProjectId::from_title_author("X", "Y").try_with_isbn13("nope");
    assert_eq!(r.unwrap_err(), IdentityError::InvalidIsbn13("nope".into()));
}

#[test]
fn with_asin_normalises_case_and_whitespace() {
    let id = ProjectId::from_title_author("X", "Y").with_asin("  b0abcdefgh  ");
    assert_eq!(id.audible_asin.as_deref(), Some("B0ABCDEFGH"));
}

#[test]
fn content_hash_hex_decode_accepts_mixed_case() {
    let id = ProjectId::from_title_author("X", "Y");
    let mut json = serde_json::to_value(&id).unwrap();
    // Force the hex string to uppercase to exercise the case-insensitive decoder.
    let upper = json["content_hash"].as_str().unwrap().to_ascii_uppercase();
    json["content_hash"] = serde_json::Value::String(upper);
    let back: ProjectId = serde_json::from_value(json).unwrap();
    assert_eq!(back, id);
}

#[test]
fn project_id_serde_round_trip() {
    let id = ProjectId::from_title_author("海辺のカフカ", "村上春樹")
        .with_asin("B0XXXX")
        .with_isbn13("9784101001012");
    let json = serde_json::to_string(&id).unwrap();
    let back: ProjectId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}
