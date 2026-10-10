mod support;

use skel::discover_files;
use skel::walker::{
    build_focus_globset, discover_files_with_filters, discover_files_with_limit,
    get_language_by_extension, should_ignore, DiscoveryError, DEFAULT_IGNORES,
};
use std::error::Error;
use std::fs;
use std::path::Path;
use support::Fixture;

#[test]
fn every_supported_extension_maps_to_the_expected_language() {
    for (extension, language) in [
        ("py", "python"),
        ("ts", "typescript"),
        ("tsx", "typescript"),
        ("js", "javascript"),
        ("jsx", "javascript"),
        ("go", "go"),
        ("rs", "rust"),
        ("java", "java"),
        ("c", "c"),
        ("h", "c"),
        ("cpp", "cpp"),
        ("hpp", "cpp"),
        ("cc", "cpp"),
        ("cxx", "cpp"),
    ] {
        assert_eq!(
            get_language_by_extension(extension),
            Some(language),
            "{extension}"
        );
    }
    for extension in [
        "", "txt", "md", "json", "yaml", "lock", "pyc", "rs.bak", "PY", ".py",
    ] {
        assert_eq!(get_language_by_extension(extension), None, "{extension}");
    }
}

#[test]
fn discovery_normalizes_extension_case_and_sorts_complete_metadata() {
    let fixture = Fixture::new();
    let cases = [
        ("z.PY", "python"),
        ("src/Upper.TsX", "typescript"),
        ("a.JSX", "javascript"),
        ("b.GO", "go"),
        ("c.RS", "rust"),
        ("d.JAVA", "java"),
        ("e.C", "c"),
        ("f.H", "c"),
        ("g.CPP", "cpp"),
        ("h.HPP", "cpp"),
        ("i.CC", "cpp"),
        ("j.CXX", "cpp"),
    ];
    for (path, _) in cases {
        fixture.write(path, "");
    }
    for path in [
        "ignored.txt",
        "ignored.py.bak",
        "no_extension",
        "src/ignored.json",
    ] {
        fixture.write(path, "");
    }
    fs::create_dir(fixture.0.join("directory.py")).unwrap();
    let before = fixture.snapshot();
    let files = discover_files(&fixture.0, &[]).unwrap();
    let mut expected = cases.to_vec();
    expected.sort_by_key(|(path, _)| *path);
    assert_eq!(files.len(), expected.len());
    for (file, (path, language)) in files.iter().zip(expected) {
        assert_eq!(file.relative_path, path);
        assert_eq!(file.lang, language);
        assert_eq!(file.absolute_path, fixture.0.join(path));
        assert!(!file.is_focus);
    }
    assert_eq!(fixture.snapshot(), before);
}

#[test]
fn every_default_ignore_and_hidden_child_is_excluded_before_recursing() {
    let fixture = Fixture::new();
    fixture.write("visible.py", "");
    fixture.write("src/visible.ts", "");
    for name in DEFAULT_IGNORES
        .iter()
        .copied()
        .chain([".custom", ".hidden_directory"])
    {
        fixture.write(&format!("{name}/deep/ignored.py"), "");
        fixture.write(&format!("src/{name}/deep/ignored.ts"), "");
        assert!(should_ignore(Path::new(&format!("src/{name}/deep/api.py"))));
    }
    fixture.write("src/.hidden.py", "");
    let paths = discover_files(&fixture.0, &["**/*".into()])
        .unwrap()
        .into_iter()
        .map(|f| f.relative_path)
        .collect::<Vec<_>>();
    assert_eq!(paths, ["src/visible.ts", "visible.py"]);
}

#[test]
fn ignore_names_match_complete_components_without_case_folding() {
    for path in [
        ".",
        "..",
        "./src/api.py",
        "targeted/api.py",
        "src/my_target/api.py",
        "src/Node_Modules/api.py",
        "src/version.1/api.py",
        "src/venv.py",
    ] {
        assert!(!should_ignore(Path::new(path)), "{path}");
    }
    for path in ["src/target/api.py", "src/.secret/api.py", "src/.secret.py"] {
        assert!(should_ignore(Path::new(path)), "{path}");
    }
}

#[test]
fn explicitly_selected_ignored_roots_and_hidden_files_are_scannable() {
    let fixture = Fixture::new();
    for directory in [".hidden", "target", "node_modules"] {
        fixture.write(&format!("{directory}/api.py"), "");
        let files = discover_files(&fixture.0.join(directory), &[]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "api.py");
    }
    let file = fixture.write(".hidden.py", "");
    let files = discover_files(&file, &[".hidden.py".into()]).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, ".hidden.py");
    assert!(files[0].is_focus);
}

#[test]
fn unsupported_single_files_and_empty_directories_are_empty_successes() {
    let fixture = Fixture::new();
    assert!(discover_files(&fixture.0, &[]).unwrap().is_empty());
    let file = fixture.write("notes.md", "# notes");
    assert!(discover_files(&file, &[]).unwrap().is_empty());
    assert!(discover_files_with_limit(&file, &[], 0)
        .unwrap()
        .files
        .is_empty());
}

#[test]
fn file_limits_count_only_supported_visible_files_and_allow_exact_boundaries() {
    let fixture = Fixture::new();
    assert!(discover_files_with_limit(&fixture.0, &[], 0)
        .unwrap()
        .files
        .is_empty());
    fixture.write("notes.txt", "");
    fixture.write("target/ignored.py", "");
    assert!(discover_files_with_limit(&fixture.0, &[], 0)
        .unwrap()
        .files
        .is_empty());
    let file = fixture.write("a.py", "");
    for root in [&fixture.0, &file] {
        assert!(matches!(
            discover_files_with_limit(root, &[], 0),
            Err(DiscoveryError::FileLimit { limit: 0 })
        ));
        assert_eq!(
            discover_files_with_limit(root, &[], 1).unwrap().files.len(),
            1
        );
    }
    fixture.write("b.py", "");
    assert!(matches!(
        discover_files_with_limit(&fixture.0, &[], 1),
        Err(DiscoveryError::FileLimit { limit: 1 })
    ));
    assert_eq!(
        discover_files_with_limit(&fixture.0, &[], 2)
            .unwrap()
            .files
            .len(),
        2
    );
}

#[test]
fn focus_union_recursive_globs_and_windows_separators_use_relative_paths() {
    let fixture = Fixture::new();
    for path in [
        "api.py",
        "src/auth.py",
        "src/deep/auth.ts",
        "src/deep/model.ts",
        "tests/auth.py",
    ] {
        fixture.write(path, "");
    }
    let patterns = ["api.py", "src\\auth.py", "src/**/auth.ts"];
    let files = discover_files(&fixture.0, &patterns.map(String::from)).unwrap();
    let focused = files
        .iter()
        .filter(|f| f.is_focus)
        .map(|f| f.relative_path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(focused, ["api.py", "src/auth.py", "src/deep/auth.ts"]);
    let subdir = discover_files(&fixture.0.join("src"), &["auth.py".into()]).unwrap();
    assert_eq!(
        subdir
            .iter()
            .filter(|f| f.is_focus)
            .map(|f| f.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["auth.py"]
    );
    let single = discover_files(&fixture.0.join("src/auth.py"), &["src/auth.py".into()]).unwrap();
    assert!(
        !single[0].is_focus,
        "Single-file focus is relative to its basename"
    );
}

#[test]
fn focus_empty_duplicate_unmatched_and_brace_patterns_are_predictable() {
    assert!(build_focus_globset(&[]).unwrap().is_none());
    let set = build_focus_globset(&["src/{api,model}.py".into(), "src/{api,model}.py".into()])
        .unwrap()
        .unwrap();
    assert!(set.is_match("src/api.py"));
    assert!(set.is_match("src/model.py"));
    assert!(!set.is_match("src/other.py"));
    assert!(!set.is_match("api.py"));
    let fixture = Fixture::new();
    fixture.write("api.py", "");
    assert!(!discover_files(&fixture.0, &["missing.*".into()]).unwrap()[0].is_focus);
}

#[test]
fn invalid_focus_is_not_silently_discarded_even_on_empty_roots() {
    let fixture = Fixture::new();
    for pattern in ["[", "[a-", "{a,b"] {
        assert!(build_focus_globset(&["*.py".into(), pattern.into()]).is_err());
        let error = discover_files(&fixture.0, &[pattern.into()]).unwrap_err();
        assert!(matches!(error, DiscoveryError::InvalidFocus(_)));
        assert!(error.source().is_some());
        assert!(error.to_string().starts_with("Invalid focus pattern:"));
    }
}

#[test]
fn exclusions_preserve_focus_exceptions_and_report_first_matching_rule() {
    let fixture = Fixture::new();
    for path in [
        "core.py",
        "docs/conf.py",
        "docs/deep/example.py",
        "docs/deep/theme.py",
        "tests/test_core.py",
        "node_modules/ignored.js",
    ] {
        fixture.write(path, "");
    }
    let discovery = discover_files_with_filters(
        &fixture.0,
        &["docs/deep/example.py".into(), "node_modules/**".into()],
        &["docs\\**".into(), "tests/**".into(), "**/*.py".into()],
        10,
    )
    .unwrap();
    assert_eq!(discovery.files.len(), 1);
    assert_eq!(discovery.files[0].relative_path, "docs/deep/example.py");
    assert!(discovery.files[0].is_focus);
    let excluded: Vec<_> = discovery
        .excluded
        .iter()
        .map(|file| (file.relative_path.as_str(), file.pattern.as_str()))
        .collect();
    assert_eq!(
        excluded,
        [
            ("core.py", "**/*.py"),
            ("docs/conf.py", "docs\\**"),
            ("docs/deep/theme.py", "docs\\**"),
            ("tests/test_core.py", "tests/**"),
        ]
    );
    assert!(discovery.warnings.is_empty());
}

#[test]
fn exclusion_paths_are_relative_to_selected_directory_or_single_file_basename() {
    let fixture = Fixture::new();
    let file = fixture.write("src/api.py", "");
    fixture.write("src/model.py", "");
    let discovery = discover_files_with_filters(
        &fixture.0.join("src"),
        &[],
        &["api.py".into(), "missing/**".into()],
        10,
    )
    .unwrap();
    assert_eq!(discovery.files[0].relative_path, "model.py");
    assert_eq!(discovery.excluded[0].relative_path, "api.py");
    assert!(
        discover_files_with_filters(&file, &[], &["api.py".into()], 1)
            .unwrap()
            .files
            .is_empty()
    );
    assert_eq!(
        discover_files_with_filters(&file, &[], &["src/api.py".into()], 1)
            .unwrap()
            .files
            .len(),
        1
    );
}

#[test]
fn invalid_exclude_is_reported_even_on_empty_roots() {
    let fixture = Fixture::new();
    let error = discover_files_with_filters(&fixture.0, &[], &["[".into()], 10).unwrap_err();
    assert!(matches!(error, DiscoveryError::InvalidExclude(_)));
    assert!(error.source().is_some());
    assert!(error.to_string().starts_with("Invalid exclude pattern:"));
}

#[test]
fn scan_limits_count_excluded_files_as_well_as_retained_files() {
    let fixture = Fixture::new();
    fixture.write("a.py", "");
    fixture.write("b.py", "");
    let discovery = discover_files_with_filters(&fixture.0, &[], &["a.py".into()], 2).unwrap();
    assert_eq!(discovery.files.len(), 1);
    assert_eq!(discovery.excluded.len(), 1);
    for patterns in [vec!["a.py".into()], vec!["**".into()]] {
        assert!(matches!(
            discover_files_with_filters(&fixture.0, &[], &patterns, 1),
            Err(DiscoveryError::FileLimit { limit: 1 })
        ));
    }
}

#[test]
fn unicode_and_space_paths_are_preserved_and_slashes_are_normalized() {
    let fixture = Fixture::new();
    fixture.write("目录 有空格/模块.py", "def 函数():\n    return 1\n");
    let files = discover_files(&fixture.0, &["目录 有空格\\*.py".into()]).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "目录 有空格/模块.py");
    assert!(files[0].is_focus);
}

#[test]
fn gitignore_does_not_change_builtin_discovery_rules() {
    let fixture = Fixture::new();
    fixture.write(".gitignore", "api.py\nsrc/\n");
    fixture.write("api.py", "");
    fixture.write("src/model.ts", "");
    assert_eq!(discover_files(&fixture.0, &[]).unwrap().len(), 2);
}

#[test]
fn discovery_errors_expose_useful_messages_without_spurious_sources() {
    let error = DiscoveryError::FileLimit { limit: 17 };
    assert_eq!(
        error.to_string(),
        "Source file count exceeds 17. Select a smaller directory."
    );
    assert!(error.source().is_none());
    let error = DiscoveryError::Scan("specific scan failure".into());
    assert_eq!(error.to_string(), "specific scan failure");
    assert!(error.source().is_none());
}

#[cfg(unix)]
#[test]
fn file_symlinks_are_included_and_directory_symlinks_are_not_followed() {
    let fixture = Fixture::new();
    fixture.write("src/api.py", "");
    std::os::unix::fs::symlink("src/api.py", fixture.0.join("alias.py")).unwrap();
    std::os::unix::fs::symlink("src", fixture.0.join("alias_directory")).unwrap();
    let paths = discover_files(&fixture.0, &[])
        .unwrap()
        .into_iter()
        .map(|f| f.relative_path)
        .collect::<Vec<_>>();
    assert_eq!(paths, ["alias.py", "src/api.py"]);
}

#[cfg(unix)]
#[test]
fn broken_links_preserve_partial_results_but_strict_discovery_returns_error() {
    let fixture = Fixture::new();
    fixture.write("api.py", "");
    std::os::unix::fs::symlink("missing.py", fixture.0.join("broken.py")).unwrap();
    let discovery = discover_files_with_limit(&fixture.0, &[], 10).unwrap();
    assert_eq!(discovery.files.len(), 1);
    assert_eq!(discovery.warnings.len(), 1);
    assert!(discovery.warnings[0].contains("broken.py"));
    assert!(matches!(
        discover_files(&fixture.0, &[]),
        Err(DiscoveryError::Scan(_))
    ));
}
