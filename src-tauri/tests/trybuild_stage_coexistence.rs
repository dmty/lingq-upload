#[test]
fn project_stage_not_interchangeable_with_event_stage() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/stage_not_interchangeable.rs");
}
