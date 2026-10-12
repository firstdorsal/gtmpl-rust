//! The shapes `#[derive(Gtmpl)]` refuses. A tuple struct used to compile and produce
//! an empty object, dropping every field without a word, which is why it is checked
//! here rather than trusted.

#[test]
fn rejected_shapes_do_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/*.rs");
}
