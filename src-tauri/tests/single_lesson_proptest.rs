use lingq_upload_lib::core::epub::Chapter;
use lingq_upload_lib::core::lesson::{single_lesson_concat, SINGLE_LESSON_SEP};
use proptest::prelude::*;

fn ch(order: usize, body: &str) -> Chapter {
    Chapter {
        order,
        title: format!("c{order}"),
        body: body.to_string(),
        ..Default::default()
    }
}

#[test]
fn empty_slice_yields_empty_string() {
    assert_eq!(single_lesson_concat(&[]), "");
}

#[test]
fn single_chapter_is_body_clone() {
    let chapters = [ch(0, "hello")];
    assert_eq!(single_lesson_concat(&chapters), "hello");
}

#[test]
fn three_chapters_joined_with_sep() {
    let chapters = [ch(0, "a"), ch(1, "b"), ch(2, "c")];
    assert_eq!(
        single_lesson_concat(&chapters),
        format!("a{SINGLE_LESSON_SEP}b{SINGLE_LESSON_SEP}c")
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    #[test]
    fn length_invariant(bodies in proptest::collection::vec("[a-zA-Z0-9]{0,80}", 0..30)) {
        let chapters: Vec<Chapter> = bodies
            .iter()
            .enumerate()
            .map(|(i, b)| ch(i, b))
            .collect();
        let out = single_lesson_concat(&chapters);
        let expected_len = bodies.iter().map(|b| b.len()).sum::<usize>()
            + SINGLE_LESSON_SEP.len() * chapters.len().saturating_sub(1);
        prop_assert_eq!(out.len(), expected_len);
        for b in &bodies {
            prop_assert!(out.contains(b));
        }
    }
}
