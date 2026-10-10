mod support;

use skel::{compute_stats, estimate_tokens, format_markdown, format_terminal_report, FileResult};
use std::fs;
use support::{assert_success, cli_command, run_cli, run_command, stderr, stdout, Fixture};

fn result(path: &str, language: &str, mode: &str, source: &str, content: &str) -> FileResult {
    FileResult {
        path: path.into(),
        lang: language.into(),
        mode: mode.into(),
        orig_tokens: estimate_tokens(source),
        skel_tokens: estimate_tokens(content),
        content: content.into(),
    }
}

#[test]
fn help_and_version_flags_exit_successfully_without_touching_the_target() {
    let fixture = Fixture::new();
    let missing = fixture.0.join("missing");
    for flag in ["--help", "-h"] {
        let output = run_cli(&missing, &[flag]);
        assert_success(&output);
        for expected in ["Usage:", "--focus", "--output", "--stats", "--mcp"] {
            assert!(stdout(&output).contains(expected));
        }
        assert!(output.stderr.is_empty());
    }
    for flag in ["--version", "-V"] {
        let output = run_cli(&missing, &[flag]);
        assert_success(&output);
        assert_eq!(
            stdout(&output),
            format!("skel {}\n", env!("CARGO_PKG_VERSION"))
        );
        assert!(output.stderr.is_empty());
    }
    assert!(fixture.snapshot().is_empty());
}

#[test]
fn omitted_target_defaults_to_the_child_working_directory() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def f():\n    return 123\n");
    let before = fixture.snapshot();
    let mut command = cli_command();
    command.current_dir(&fixture.0).arg("--stats");
    let output = run_command(command);
    assert_success(&output);
    assert!(stdout(&output).contains("Files processed:    1"));
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn missing_target_reports_error_and_preserves_existing_output() {
    let fixture = Fixture::new();
    let destination = fixture.write("context.md", "existing context");
    let before = fixture.snapshot();
    let output = run_cli(
        &fixture.0.join("missing"),
        &["-o", destination.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("does not exist"));
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn invalid_arguments_have_usage_errors_and_do_not_write_files() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def f():\n    return 1\n");
    let before = fixture.snapshot();
    for arguments in [
        vec!["--unknown"],
        vec!["--focus"],
        vec!["--output"],
        vec!["--stats=maybe"],
        vec!["extra", "another"],
    ] {
        let output = run_cli(&fixture.0, &arguments);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{arguments:?}: {}",
            stderr(&output)
        );
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains("error:"));
        assert_eq!(fixture.snapshot(), before);
    }
}

#[test]
fn empty_and_unsupported_targets_do_not_overwrite_the_requested_output() {
    let fixture = Fixture::new();
    let destination = fixture.write("context.md", "keep me");
    let unsupported = fixture.write("notes.txt", "notes");
    for root in [&fixture.0, &unsupported] {
        let output = run_cli(root, &["-o", destination.to_str().unwrap()]);
        assert_success(&output);
        assert_eq!(
            stdout(&output),
            "No supported source files found in target directory.\n"
        );
        assert!(output.stderr.is_empty());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "keep me");
    }
}

#[test]
fn statistics_only_produces_the_exact_report_and_no_output_file() {
    let fixture = Fixture::new();
    let source = "def api():\n    return 123456\n";
    fixture.write("api.py", source);
    let destination = fixture.write("context.md", "keep me");
    let before = fixture.snapshot();
    let expected = result(
        "api.py",
        "python",
        "SKELETON",
        source,
        "def api():\n    ...\n",
    );
    for flag in ["--stats", "-s"] {
        let output = run_cli(&fixture.0, &[flag, "-o", destination.to_str().unwrap()]);
        assert_success(&output);
        assert_eq!(
            stdout(&output),
            format_terminal_report(
                &compute_stats(std::slice::from_ref(&expected)),
                env!("CARGO_PKG_VERSION")
            )
        );
        assert!(output.stderr.is_empty());
        assert_eq!(fixture.snapshot(), before);
    }
}

#[test]
fn mixed_language_cli_output_matches_all_expected_files_and_statistics() {
    let fixture = Fixture::new();
    let destination = Fixture::new();
    let cases = [
        (
            "a.py",
            "python",
            "def api():\n    return 123456\n",
            "def api():\n    ...\n",
        ),
        (
            "b.ts",
            "typescript",
            "export function api() { return 123456; }\n",
            "export function api() { /* ... */ }\n",
        ),
        (
            "c.JSX",
            "javascript",
            "const View = () => <div>secret</div>;\n",
            "const View = () => { /* ... */ };\n",
        ),
        (
            "d.TSX",
            "typescript",
            "const View = () => <div>secret</div>;\n",
            "const View = () => { /* ... */ };\n",
        ),
        (
            "e.rs",
            "rust",
            "pub fn api() { let value = 123456; }\n",
            "pub fn api() { /* ... */ }\n",
        ),
        (
            "f.go",
            "go",
            "func api() { return 123456 }\n",
            "func api() { /* ... */ }\n",
        ),
        (
            "g.java",
            "java",
            "class Api { int get() { return 123456; } }\n",
            "class Api { int get() { /* ... */ } }\n",
        ),
        (
            "h.c",
            "c",
            "int api() { return 123456; }\n",
            "int api() { /* ... */ }\n",
        ),
        (
            "i.cpp",
            "cpp",
            "int api() { return 123456; }\n",
            "int api() { /* ... */ }\n",
        ),
    ];
    let mut results = Vec::new();
    for (path, language, source, expected) in cases {
        fixture.write(path, source);
        results.push(result(path, language, "SKELETON", source, expected));
    }
    fixture.write("node_modules/ignored.js", "SECRET");
    fixture.write("notes.txt", "SECRET");
    let before = fixture.snapshot();
    let output_path = destination.0.join("context.md");
    let output = run_cli(&fixture.0, &["--output", output_path.to_str().unwrap()]);
    assert_success(&output);
    let stats = compute_stats(&results);
    let expected = format_markdown(&results, &stats);
    assert_eq!(fs::read_to_string(&output_path).unwrap(), expected);
    assert_eq!(
        stdout(&output),
        format!(
            "{}[OK] Context written to: {} ({} bytes)\n",
            format_terminal_report(&stats, env!("CARGO_PKG_VERSION")),
            output_path.display(),
            skel::formatter::format_thousands(expected.len())
        )
    );
    assert!(output.stderr.is_empty());
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn repeated_focus_flags_form_a_union_and_preserve_exact_unicode_source() {
    let fixture = Fixture::new();
    let destination = Fixture::new();
    let sources = [
        "def a():\r\n    return '中文 🐾'\r\n",
        "def b():\n    return 123456\n",
        "def c():\n    return 654321\n",
    ];
    for (path, source) in ["a.py", "b.py", "c.py"].into_iter().zip(sources) {
        fixture.write(path, source);
    }
    let output_path = destination.0.join("context.md");
    let output = run_cli(
        &fixture.0,
        &[
            "-f",
            "a.py",
            "--focus",
            "b.py",
            "-f",
            "a.py",
            "-o",
            output_path.to_str().unwrap(),
        ],
    );
    assert_success(&output);
    let expected = [
        result("a.py", "python", "FULL", sources[0], sources[0]),
        result("b.py", "python", "FULL", sources[1], sources[1]),
        result(
            "c.py",
            "python",
            "SKELETON",
            sources[2],
            "def c():\n    ...\n",
        ),
    ];
    assert_eq!(
        fs::read_to_string(output_path).unwrap(),
        format_markdown(&expected, &compute_stats(&expected))
    );
}

#[test]
fn single_file_focus_matches_the_basename_and_not_its_parent_directory() {
    let fixture = Fixture::new();
    let source = "def api():\n    return 42\n";
    let file = fixture.write("src/api.py", source);
    for (pattern, mode, content) in [
        ("api.py", "FULL", source),
        ("src/api.py", "SKELETON", "def api():\n    ...\n"),
    ] {
        let output = run_cli(&file, &["--focus", pattern]);
        assert_success(&output);
        assert!(stdout(&output).contains(&format!("### File: `api.py` ({mode})")));
        assert!(stdout(&output).contains(content.trim_end()));
    }
}

#[test]
fn malformed_source_is_retained_without_pruning_other_valid_files() {
    let fixture = Fixture::new();
    let destination = Fixture::new();
    let broken = "def broken(\n    return 'retain this'\n";
    let valid = "def valid():\n    return 'remove this'\n";
    fixture.write("broken.py", broken);
    fixture.write("valid.py", valid);
    let output_path = destination.0.join("context.md");
    let output = run_cli(&fixture.0, &["-o", output_path.to_str().unwrap()]);
    assert_success(&output);
    // The CLI labels non-focus files SKELETON even when parsing retains their input.
    let results = [
        result("broken.py", "python", "SKELETON", broken, broken),
        result(
            "valid.py",
            "python",
            "SKELETON",
            valid,
            "def valid():\n    ...\n",
        ),
    ];
    assert_eq!(
        fs::read_to_string(output_path).unwrap(),
        format_markdown(&results, &compute_stats(&results))
    );
}

#[test]
fn non_utf8_files_are_skipped_and_do_not_pollute_statistics_or_output() {
    let fixture = Fixture::new();
    let destination = Fixture::new();
    fixture.write_bytes("binary.py", &[0xff, 0xfe, 0x80]);
    fixture.write("api.py", "def api():\n    return 123\n");
    let output_path = destination.0.join("context.md");
    let output = run_cli(&fixture.0, &["-o", output_path.to_str().unwrap()]);
    assert_success(&output);
    let markdown = fs::read_to_string(output_path).unwrap();
    assert!(!markdown.contains("binary.py"));
    assert!(markdown.contains("### File: `api.py`"));
    assert!(stdout(&output).contains("Files processed:    1"));
}

#[test]
fn empty_source_files_are_included_with_zero_tokens() {
    let fixture = Fixture::new();
    fixture.write("empty.py", "");
    let output = run_cli(&fixture.0, &[]);
    assert_success(&output);
    assert!(stdout(&output).contains("Files processed:    1"));
    assert!(stdout(&output).contains("Original tokens:    0"));
    assert!(stdout(&output).contains("| `empty.py` | **SKELETON** | python | 0 |"));
    assert!(!stdout(&output).contains("NaN"));
}

#[test]
fn short_preview_contains_the_complete_markdown_without_a_truncation_notice() {
    let fixture = Fixture::new();
    let source = "def api():\n    return 1\n";
    fixture.write("api.py", source);
    let output = run_cli(&fixture.0, &[]);
    assert_success(&output);
    let preview = stdout(&output)
        .split_once("--- Preview (first 40 lines, use -o context.md to save) ---\n")
        .unwrap()
        .1;
    let results = [result(
        "api.py",
        "python",
        "SKELETON",
        source,
        "def api():\n    ...\n",
    )];
    assert_eq!(
        preview,
        format!(
            "{}\n",
            format_markdown(&results, &compute_stats(&results))
                .lines()
                .collect::<Vec<_>>()
                .join("\n")
        )
    );
    assert!(!stdout(&output).contains("[Truncated."));
}

#[test]
fn long_preview_is_limited_to_exactly_forty_markdown_lines() {
    let fixture = Fixture::new();
    let source = (0..80)
        .map(|i| format!("VALUE_{i} = {i}\n"))
        .collect::<String>();
    fixture.write("values.py", &source);
    let output = run_cli(&fixture.0, &[]);
    assert_success(&output);
    let preview = stdout(&output)
        .split_once("--- Preview (first 40 lines, use -o context.md to save) ---\n")
        .unwrap()
        .1;
    let results = [result("values.py", "python", "SKELETON", &source, &source)];
    let expected_lines = format_markdown(&results, &compute_stats(&results))
        .lines()
        .take(40)
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        preview,
        format!(
            "{expected_lines}\n...\n[Truncated. Use `-o context.md` to output full markdown]\n"
        )
    );
    assert!(!preview.contains("VALUE_79"));
}

#[test]
fn write_failures_return_nonzero_and_leave_sources_unchanged() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 1\n");
    let before = fixture.snapshot();
    for destination in [fixture.0.join("missing/context.md"), fixture.0.clone()] {
        let output = run_cli(&fixture.0, &["-o", destination.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1));
        assert!(stderr(&output).contains("Error writing to"));
        assert!(!stdout(&output).contains("[OK]"));
        assert_eq!(fixture.snapshot(), before);
    }
}

#[test]
fn unicode_and_space_input_output_paths_work_without_shell_interpretation() {
    let fixture = Fixture::new();
    let file = fixture.write("目录 有空格/模块.py", "def 函数():\n    return '中文'\n");
    let output_path = fixture.0.join("结果 有空格.md");
    let output = run_cli(&file, &["-o", output_path.to_str().unwrap()]);
    assert_success(&output);
    let markdown = fs::read_to_string(output_path).unwrap();
    assert!(markdown.contains("`模块.py`"));
    assert!(markdown.contains("def 函数():\n    ..."));
}

#[test]
fn invalid_focus_preserves_existing_output_instead_of_overwriting_it() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 1\n");
    let output_path = fixture.write("context.md", "existing output");
    let before = fixture.snapshot();
    let output = run_cli(
        &fixture.0,
        &["--focus", "[", "-o", output_path.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("Invalid --focus pattern"));
    assert!(output.stdout.is_empty());
    assert_eq!(fixture.snapshot(), before);
}

#[cfg(unix)]
#[test]
fn partial_scan_errors_warn_and_complete_failure_returns_nonzero() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 1\n");
    std::os::unix::fs::symlink("missing.py", fixture.0.join("broken.py")).unwrap();
    let partial = run_cli(&fixture.0, &["--stats"]);
    assert_success(&partial);
    assert!(stderr(&partial).contains("Warning:") && stderr(&partial).contains("broken.py"));
    assert!(stdout(&partial).contains("Files processed:    1"));
    fs::remove_file(fixture.0.join("api.py")).unwrap();
    let failed = run_cli(&fixture.0, &["--stats"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    assert!(stderr(&failed).contains("broken.py"));
}
