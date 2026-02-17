#[test]
fn help_doc_is_up_to_date() {
    let expected = clap_markdown::help_markdown::<detail_cli::Cli>();
    let current = std::fs::read_to_string("docs/HELP.md").expect("failed to read docs/HELP.md");
    assert_eq!(
        current.trim(),
        expected.trim(),
        "docs/HELP.md is out of date. \
         Run `cargo run --example generate_help > docs/HELP.md` to regenerate it."
    );
}
