mod support;

use skel::discover_files;
use skel::walker::{discover_files_with_limit, DiscoveryError};
use std::fs;
use support::{run_cli, Fixture};

#[test]
fn missing_scan_roots_are_errors_instead_of_empty_successes() {
    let fixture = Fixture::new();
    let missing = fixture.0.join("missing");
    assert!(matches!(
        discover_files(&missing, &[]),
        Err(DiscoveryError::Scan(_))
    ));
    assert!(matches!(
        discover_files_with_limit(&missing, &[], 1_000),
        Err(DiscoveryError::Scan(_))
    ));
}

#[test]
fn discovers_files_beneath_ignored_ancestors_but_excludes_ignored_children() {
    let fixture = Fixture::new();
    let root = fixture.0.join(".parent").join("target").join("project");
    for dir in ["", "src", "node_modules", ".hidden"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "api.py",
        "src/auth.py",
        "node_modules/ignored.js",
        ".hidden/ignored.py",
    ] {
        fs::write(root.join(file), "def api():\n    return 1\n").unwrap();
    }
    let files = discover_files(&root, &["src\\*.py".to_string()]).unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].relative_path, "api.py");
    assert!(!files[0].is_focus);
    assert_eq!(files[1].relative_path, "src/auth.py");
    assert!(files[1].is_focus);
    let single = discover_files(&root.join("api.py"), &["*.py".to_string()]).unwrap();
    assert_eq!(single.len(), 1);
    assert!(single[0].is_focus);
}

#[test]
fn rejects_invalid_focus_patterns_in_library_and_cli() {
    let fixture = Fixture::new();
    let file = fixture.0.join("api.py");
    fs::write(&file, "def api():\n    return 1\n").unwrap();
    assert!(discover_files(&file, &["[".to_string()]).is_err());
    let output_path = fixture.0.join("context.md");
    let output = run_cli(
        &file,
        &["--focus", "[", "--output", output_path.to_str().unwrap()],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid --focus pattern"));
    assert!(!output_path.exists());
}

#[test]
fn cli_focus_preserves_full_code_and_other_files_are_pruned() {
    let fixture = Fixture::new();
    for name in ["auth.py", "models.py"] {
        fs::write(fixture.0.join(name), "def api():\n    return 42\n").unwrap();
    }
    let output_path = fixture.0.join("context.md");
    let output = run_cli(
        &fixture.0,
        &[
            "--focus",
            "auth.py",
            "--output",
            output_path.to_str().unwrap(),
        ],
    );
    assert!(output.status.success());
    let markdown = fs::read_to_string(output_path).unwrap();
    assert!(markdown.contains("`auth.py` (FULL)"));
    assert!(markdown.contains("`models.py` (SKELETON)"));
    assert_eq!(markdown.matches("return 42").count(), 1);
    assert!(markdown.contains("..."));
}
