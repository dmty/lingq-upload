use lingq_upload_lib::core::identity::{content_hash, ProjectId};
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
fn project_id_serde_round_trip() {
    let id = ProjectId::from_title_author("海辺のカフカ", "村上春樹")
        .with_asin("B0XXXX")
        .with_isbn13("9784101001012");
    let json = serde_json::to_string(&id).unwrap();
    let back: ProjectId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}
