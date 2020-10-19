#[test]
fn test_cases() {
    let t = trybuild::TestCases::new();
    t.compile_fail("mula_proc_macro/tests/01-no-output.rs");
    t.compile_fail("mula_proc_macro/tests/02-no-input.rs");
    t.compile_fail("mula_proc_macro/tests/03-multiple-inputs.rs");
}