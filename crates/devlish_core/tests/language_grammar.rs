#[test]
fn case_and_empty_text_are_in_the_grammar() {
    let grammar = "docs/LANGUAGE_GRAMMAR.ebnf";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(grammar);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert!(text.contains("case_statement"), "{grammar}");
    assert!(text.contains("when_arm"));
    assert!(text.contains("emptiness_condition"));
    assert!(text.contains("\"empty\" | \"blank\""));
}
