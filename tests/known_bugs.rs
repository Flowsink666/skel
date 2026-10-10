//! Regressions for previously fixed production defects.
//! All fixes have landed; these tests run as part of the default test suite.

use skel::skeletonize;
use tree_sitter::Parser;

fn assert_valid_python(source: &str) {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    assert!(!parser.parse(source, None).unwrap().root_node().has_error());
}

#[test]
fn python_f_strings_are_implementation_expressions_not_docstrings() {
    let source = "def f():\n    f\"secret {expensive()}\"\n    return 1\n";
    assert_valid_python(source);
    assert_eq!(skeletonize(source, "python", false), "def f():\n    ...\n");
}

#[test]
fn python_bytes_literals_are_not_docstrings() {
    let source = "def f():\n    b\"secret bytes\"\n    return 1\n";
    assert_valid_python(source);
    assert_eq!(skeletonize(source, "python", false), "def f():\n    ...\n");
}

#[test]
fn python_implicitly_concatenated_docstrings_are_preserved() {
    let source = "def f():\n    \"first \" \"second\"\n    return 1\n";
    assert_valid_python(source);
    assert_eq!(
        skeletonize(source, "python", false),
        "def f():\n    \"first \" \"second\"\n    ...\n"
    );
}

#[test]
fn python_parenthesized_docstrings_are_preserved() {
    let source = "def f():\n    (\"parenthesized documentation\")\n    return 1\n";
    assert_valid_python(source);
    assert_eq!(
        skeletonize(source, "python", false),
        "def f():\n    (\"parenthesized documentation\")\n    ...\n"
    );
}

#[test]
fn javascript_generator_function_expressions_have_their_bodies_pruned() {
    let source = "const named = function* generate() { yield 123; };\nconst anonymous = function* () { yield 456; };\nconst next = 1;\n";
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .unwrap();
    assert!(!parser.parse(source, None).unwrap().root_node().has_error());
    assert_eq!(skeletonize(source, "javascript", false), "const named = function* generate() { /* ... */ };\nconst anonymous = function* () { /* ... */ };\nconst next = 1;\n");
}

#[test]
fn python_missing_function_bodies_are_retained_as_invalid_input() {
    let source = "def good():\n    return 1\ndef missing():\n";
    assert_eq!(skeletonize(source, "python", false), source);
}

fn context_for_path(path: &str) -> (String, String) {
    let file = skel::FileResult {
        path: path.into(),
        lang: "python".into(),
        mode: "FULL".into(),
        orig_tokens: 1,
        skel_tokens: 1,
        content: "x = 1".into(),
    };
    let files = [file];
    let stats = skel::compute_stats(&files);
    (
        skel::format_markdown(&files, &stats),
        skel::format_context_summary(&files, &stats),
    )
}

#[test]
fn markdown_file_paths_with_backticks_use_longer_inline_delimiters() {
    let (markdown, summary) = context_for_path("weird`name.py");
    assert!(
        summary.contains("| ``weird`name.py`` | **FULL** | python | 1 |"),
        "{summary}"
    );
    assert!(
        markdown.contains("### File: ``weird`name.py`` (FULL)"),
        "{markdown}"
    );
}

#[test]
fn file_paths_with_pipes_are_escaped_in_log_summary_tables() {
    let (markdown, summary) = context_for_path("api|internal.py");
    assert!(
        summary.contains("| `api\\|internal.py` | **FULL** | python | 1 |"),
        "{summary}"
    );
    assert!(
        markdown.contains("### File: `api|internal.py` (FULL)"),
        "{markdown}"
    );
}
