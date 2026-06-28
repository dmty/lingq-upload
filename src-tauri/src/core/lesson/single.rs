use crate::core::epub::Chapter;

pub const SINGLE_LESSON_SEP: &str = "\n\n";

/// Concatenate chapter bodies into one lesson text.
///
/// Invariant (proptest-enforced):
/// `concat(c).len() == sum(c.body.len()) + SEP.len() * (n - 1)`
/// for `n = c.len().saturating_sub(1)`.
pub fn concat(chapters: &[Chapter]) -> String {
    if chapters.is_empty() {
        return String::new();
    }
    if chapters.len() == 1 {
        return chapters[0].body.clone();
    }
    let total = chapters.iter().map(|c| c.body.len()).sum::<usize>()
        + SINGLE_LESSON_SEP.len() * (chapters.len() - 1);
    let mut out = String::with_capacity(total);
    for (i, c) in chapters.iter().enumerate() {
        if i > 0 {
            out.push_str(SINGLE_LESSON_SEP);
        }
        out.push_str(&c.body);
    }
    out
}
