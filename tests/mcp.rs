mod support;

use serde_json::{json, Value};
use std::fs;
use support::mcp::{markdown, success, tool_error, Client};
use support::{run_cli, Fixture};

#[test]
fn stdio_handshake_discovery_and_clean_shutdown() {
    let fixture = Fixture::new();
    fixture.write("src/api.py", "def api():\n    return 42\n");
    fixture.write("node_modules/ignored.js", "const x = 1;");
    let mut client = Client::new(&fixture);
    let response = client.request("tools/list", json!({}));
    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    for name in ["skel_list_files", "skel_skeletonize"] {
        let tool = tools.iter().find(|tool| tool["name"] == name).unwrap();
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert!(tool["inputSchema"]["properties"]["path"].is_object());
        assert!(tool["inputSchema"]["properties"]["focus"].is_object());
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["destructiveHint"], false);
        assert_eq!(tool["annotations"]["idempotentHint"], true);
        assert_eq!(tool["annotations"]["openWorldHint"], false);
        assert_eq!(tool["inputSchema"]["properties"]["path"]["type"], "string");
        assert_eq!(tool["inputSchema"]["properties"]["focus"]["type"], "array");
        assert_eq!(
            tool["inputSchema"]["properties"]["focus"]["items"]["type"],
            "string"
        );
    }
    let listing = client.call("skel_list_files", json!({}));
    let data = &listing["result"]["structuredContent"];
    assert_eq!(data["file_count"], 1);
    assert_eq!(data["files"][0]["path"], "src/api.py");
    client.close();
}

#[test]
fn deeply_nested_sources_do_not_crash_the_server() {
    let fixture = Fixture::new();
    let depth = 30_000;
    for (name, declaration, function) in [
        ("deep.py", "value = ", "def after():\n    return 99\n"),
        (
            "deep.js",
            "const value = ",
            "function after() { return 99; }\n",
        ),
    ] {
        let source = format!(
            "{declaration}{}0{}\n{function}",
            "[".repeat(depth),
            "]".repeat(depth)
        );
        fixture.write(name, &source);
    }
    fixture.write("api.py", "def api():\n    return 42\n");
    let mut client = Client::new(&fixture);
    for name in ["deep.py", "deep.js"] {
        let context = client.call("skel_skeletonize", json!({"path": name}));
        assert_ne!(context["result"]["isError"], true, "{context}");
        assert_eq!(
            context["result"]["structuredContent"]["files"][0]["mode"],
            "SKELETON"
        );
        let markdown = context["result"]["content"][0]["text"].as_str().unwrap();
        assert!(markdown.contains("after()"));
        assert!(!markdown.contains("return 99"));
        assert_eq!(
            context["result"]["structuredContent"]["stats"]["file_count"],
            1
        );
        assert!(client.request("ping", json!({}))["result"].is_object());
    }
    let normal = client.call("skel_skeletonize", json!({"path": "api.py"}));
    let markdown = normal["result"]["content"][0]["text"].as_str().unwrap();
    assert!(markdown.contains("def api():"));
    assert!(!markdown.contains("return 42"));
    client.close();
}

#[test]
fn context_focus_stats_warnings_and_markdown_fences() {
    let fixture = Fixture::new();
    let auth = "def auth():\n    return 42\n";
    fixture.write("src/auth.py", auth);
    fixture.write("src/models.py", "def model():\n    return 43\n");
    fixture.write("src/broken.py", "def broken(\n    return 99\n");
    fixture.write(
        "src/view.tsx",
        "export const View = () => <div>secret</div>;\n",
    );
    fixture.write(
        "src/docs.py",
        "def docs():\n    \"\"\"\n```\nexample\n```\n    \"\"\"\n    return 44\n",
    );
    fs::write(fixture.0.join("src/binary.py"), [0xff, 0xfe]).unwrap();
    let mut client = Client::new(&fixture);
    let arguments = json!({"path": "src", "focus": ["auth.py"]});
    let context = client.call("skel_skeletonize", arguments);
    assert_ne!(context["result"]["isError"], true, "{context}");
    let markdown = context["result"]["content"][0]["text"].as_str().unwrap();
    assert!(markdown.contains("`auth.py` (FULL)"));
    assert!(markdown.contains("return 42"));
    assert!(markdown.contains("`models.py` (SKELETON)"));
    assert!(!markdown.contains("return 43"));
    assert!(!markdown.contains("secret"));
    assert!(markdown.contains("`broken.py` (UNCHANGED)"));
    assert!(markdown.contains("````python\ndef docs():"));
    let data = &context["result"]["structuredContent"];
    assert_eq!(data["stats"]["file_count"], 5);
    assert_eq!(data["warnings"].as_array().unwrap().len(), 1);
    assert!(data["warnings"][0].as_str().unwrap().contains("UTF-8"));
    assert_eq!(
        data["markdown_estimated_tokens"],
        skel::estimate_tokens(markdown)
    );
    assert!(data["stats"]["reduction_percent"].as_f64().unwrap() > 0.0);
    assert_eq!(
        fs::read_to_string(fixture.0.join("src/auth.py")).unwrap(),
        auth
    );
    let single = client.call("skel_skeletonize", json!({"path": "src/auth.py"}));
    assert_eq!(
        single["result"]["structuredContent"]["stats"]["file_count"],
        1
    );
    client.close();
}

#[test]
fn errors_are_reported_and_server_remains_usable() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    fixture.write("api.py", "def api():\n    return 1\n");
    outside.write("secret.py", "def secret():\n    return 2\n");
    let mut client = Client::new(&fixture);
    for arguments in [
        json!({"path": ".."}),
        json!({"path": outside.0.join("secret.py")}),
        json!({"path": "missing.py"}),
        json!({"focus": ["["]}),
    ] {
        let response = client.call("skel_skeletonize", arguments);
        assert_eq!(response["result"]["isError"], true, "{response}");
        assert!(response["result"]["content"][0]["text"].is_string());
    }
    let invalid = client.call("skel_skeletonize", json!({"path": 123}));
    // The SDK reports schema validation failures as visible tool errors.
    assert_eq!(invalid["result"]["isError"], true, "{invalid}");
    assert!(invalid["result"]["content"][0]["text"].is_string());
    let unknown = client.call("skel_stats", json!({}));
    assert!(unknown["error"].is_object());
    fixture.write("huge.py", &"x".repeat(2 * 1024 * 1024 + 1));
    let huge = client.call("skel_skeletonize", json!({"path": "huge.py"}));
    assert_eq!(huge["result"]["isError"], true);
    let stats = client.call("skel_skeletonize", json!({"path": "api.py"}));
    assert_eq!(
        stats["result"]["structuredContent"]["stats"]["file_count"],
        1
    );
    assert!(client.request("ping", json!({}))["result"].is_object());
    client.close();
}

#[test]
fn mcp_rejects_conflicting_cli_flags_and_file_roots() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 1\n");
    for arguments in [
        vec!["--stats"],
        vec!["--focus", "*.py"],
        vec!["--output", "context.md"],
    ] {
        let mut flags = vec!["--mcp"];
        flags.extend(arguments);
        let output = run_cli(&fixture.0, &flags);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let output = run_cli(&fixture.0.join("api.py"), &["--mcp"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must be a directory"));
}

#[test]
fn empty_inputs_unreadable_inputs_and_resource_limits() {
    let fixture = Fixture::new();
    let mut client = Client::new(&fixture);
    let empty = client.call("skel_skeletonize", json!({}));
    assert_eq!(
        empty["result"]["structuredContent"]["stats"]["file_count"],
        0
    );
    fs::write(fixture.0.join("binary.py"), [0xff, 0xfe]).unwrap();
    let unreadable = client.call("skel_skeletonize", json!({"path": "binary.py"}));
    assert_eq!(unreadable["result"]["isError"], true);

    // A file within the source limit can still exceed the response limit once
    // the Markdown wrapper and metadata are included; return an error, not a cut.
    let large_source = format!("value = \"{}\"\n", "x".repeat(2 * 1024 * 1024 - 11));
    assert_eq!(large_source.len(), 2 * 1024 * 1024);
    fixture.write("large.py", &large_source);
    let large = client.call("skel_skeletonize", json!({"path": "large.py"}));
    assert_eq!(large["result"]["isError"], true);
    assert!(large["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Response"));
    for i in 0..9 {
        fixture.write(&format!("big/{i}.py"), &large_source);
    }
    let total = client.call("skel_skeletonize", json!({"path": "big"}));
    assert_eq!(total["result"]["isError"], true);
    assert!(total["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("16 MiB"));
    for i in 0..1_000 {
        fixture.write(&format!("many/{i}.py"), "");
    }
    let exact = client.call("skel_list_files", json!({"path": "many"}));
    assert_eq!(exact["result"]["structuredContent"]["file_count"], 1_000);
    let exact_context = client.call("skel_skeletonize", json!({"path": "many"}));
    let exact_data = success(&exact_context);
    assert_eq!(exact_data["stats"]["file_count"], 1_000);
    assert_eq!(exact_data["files"].as_array().unwrap().len(), 1_000);
    assert_eq!(exact_data["stats"]["original_estimated_tokens"], 0);
    assert_eq!(exact_data["stats"]["skeleton_estimated_tokens"], 0);
    fixture.write("many/extra.py", "");
    for tool in ["skel_list_files", "skel_skeletonize"] {
        let many = client.call(tool, json!({"path": "many"}));
        assert_eq!(many["result"]["isError"], true);
        assert!(many["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("1000"));
        assert!(client.request("ping", json!({}))["result"].is_object());
    }
    client.close();
}

#[cfg(unix)]
#[test]
fn scan_failures_are_visible_in_both_tools_and_empty_scans_are_errors() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 42\n");
    let broken = fixture.0.join("broken.py");
    std::os::unix::fs::symlink("missing.py", &broken).unwrap();
    let mut client = Client::new(&fixture);
    for tool in ["skel_list_files", "skel_skeletonize"] {
        let response = client.call(tool, json!({}));
        assert_ne!(response["result"]["isError"], true, "{response}");
        let data = &response["result"]["structuredContent"];
        assert_eq!(data["files"].as_array().unwrap().len(), 1);
        assert_eq!(data["warnings"].as_array().unwrap().len(), 1);
        assert!(data["warnings"][0].as_str().unwrap().contains("broken.py"));
    }
    fs::remove_file(fixture.0.join("api.py")).unwrap();
    for tool in ["skel_list_files", "skel_skeletonize"] {
        let response = client.call(tool, json!({}));
        assert_eq!(response["result"]["isError"], true, "{response}");
        assert!(client.request("ping", json!({}))["result"].is_object());
    }
    fs::remove_file(broken).unwrap();
    client.close();
}

#[cfg(unix)]
#[test]
fn file_symlinks_cannot_escape_server_root() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    outside.write("secret.py", "def secret():\n    return 2\n");
    std::os::unix::fs::symlink(outside.0.join("secret.py"), fixture.0.join("link.py")).unwrap();
    let mut client = Client::new(&fixture);
    for tool in ["skel_list_files", "skel_skeletonize"] {
        for arguments in [json!({}), json!({"path": "link.py"})] {
            tool_error(&client.call(tool, arguments), "outside");
        }
    }
    client.close();
}

#[test]
fn initialization_advertises_protocol_identity_and_read_only_instructions() {
    let fixture = Fixture::new();
    let mut client = Client::new(&fixture);
    assert_eq!(client.initialization["protocolVersion"], "2025-11-25");
    assert!(client.initialization["serverInfo"]["version"]
        .as_str()
        .is_some_and(|version| !version.is_empty()));
    let instructions = client.initialization["instructions"].as_str().unwrap();
    assert!(instructions.contains("Read-only"));
    assert!(instructions.contains("startup directory"));
    assert!(instructions.contains("UNCHANGED"));
    assert!(client.request("ping", json!({}))["result"].is_object());
    client.close();
}

#[test]
fn listing_is_sorted_normalizes_paths_and_focus_is_relative_to_selected_input() {
    let fixture = Fixture::new();
    fixture.write("src/z.py", "def z():\n    return 1\n");
    fixture.write("src/nested/a.TSX", "const View = () => <div/>;\n");
    fixture.write("src/.hidden.py", "secret");
    fixture.write("src/target/ignored.rs", "secret");
    fixture.write("src/notes.txt", "notes");
    let before = fixture.snapshot();
    let mut client = Client::new(&fixture);
    let response = client.call(
        "skel_list_files",
        json!({"path": "src", "focus": ["nested\\*.TSX", "z.py", "z.py"]}),
    );
    assert_eq!(
        success(&response),
        &json!({
            "file_count": 2,
            "files": [
                {"path": "nested/a.TSX", "language": "typescript", "focus": true},
                {"path": "z.py", "language": "python", "focus": true}
            ], "warnings": []
        })
    );
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap(),
        *success(&response)
    );
    let single = client.call(
        "skel_list_files",
        json!({"path": "src/z.py", "focus": ["z.py"]}),
    );
    assert_eq!(
        success(&single)["files"],
        json!([{"path": "z.py", "language": "python", "focus": true}])
    );
    let not_focused = client.call(
        "skel_list_files",
        json!({"path": "src/z.py", "focus": ["src/z.py"]}),
    );
    assert_eq!(success(&not_focused)["files"][0]["focus"], false);
    assert_eq!(fixture.snapshot(), before);
    client.close();
}

#[test]
fn both_tools_validate_all_argument_types_and_reject_unknown_fields() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 1\n");
    let before = fixture.snapshot();
    let mut client = Client::new(&fixture);
    for tool in ["skel_list_files", "skel_skeletonize"] {
        for arguments in [
            json!({"path": null}),
            json!({"path": 123}),
            json!({"path": []}),
            json!({"focus": null}),
            json!({"focus": "*.py"}),
            json!({"focus": [123]}),
            json!({"focus": [null]}),
            json!({"extra": true}),
        ] {
            tool_error(&client.call(tool, arguments.clone()), "");
            assert!(
                client.request("ping", json!({}))["result"].is_object(),
                "{arguments}"
            );
        }
        for arguments in [
            json!({"focus": ["["]}),
            json!({"path": "missing.py"}),
            json!({"path": ".."}),
        ] {
            tool_error(&client.call(tool, arguments), "");
        }
        success(&client.call(tool, json!({})));
    }
    assert_eq!(fixture.snapshot(), before);
    client.close();
}

#[test]
fn omitted_or_null_argument_envelopes_use_defaults_and_arrays_are_protocol_errors() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 1\n");
    let mut client = Client::new(&fixture);
    for tool in ["skel_list_files", "skel_skeletonize"] {
        let default = client.call(tool, json!({}));
        let omitted = client.request("tools/call", json!({"name": tool}));
        let null = client.call(tool, Value::Null);
        // The SDK deserializes a null optional argument object as omitted.
        assert_eq!(success(&omitted), success(&default));
        assert_eq!(success(&null), success(&default));
        let array = client.call(tool, json!([]));
        assert!(array["error"].is_object(), "{array}");
        assert!(array.get("result").is_none(), "{array}");
    }
    client.close();
}

#[test]
fn public_parameter_defaults_and_server_root_validation_are_explicit() {
    let params: skel::mcp::ContextParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(params.path, ".");
    assert!(params.focus.is_empty());
    assert!(serde_json::from_value::<skel::mcp::ContextParams>(json!({"unknown": 1})).is_err());
    let fixture = Fixture::new();
    assert!(skel::mcp::SkelServer::new(&fixture.0).is_ok());
    let file = fixture.write("api.py", "");
    assert!(skel::mcp::SkelServer::new(&file)
        .err()
        .unwrap()
        .contains("must be a directory"));
    assert!(skel::mcp::SkelServer::new(&fixture.0.join("missing"))
        .err()
        .unwrap()
        .contains("Cannot open server root"));
    let output = run_cli(&fixture.0.join("missing"), &["--mcp"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot open server root"));
}

#[test]
fn valid_absolute_paths_dot_segments_and_explicit_ignored_roots_stay_scoped() {
    let fixture = Fixture::new();
    fixture.write("src/api.py", "def api():\n    return 1\n");
    fixture.write("target/internal.py", "def internal():\n    return 2\n");
    let mut client = Client::new(&fixture);
    for path in [
        fixture.0.join("src/api.py").to_string_lossy().into_owned(),
        "src/../src/api.py".into(),
    ] {
        let response = client.call("skel_list_files", json!({"path": path}));
        assert_eq!(success(&response)["files"][0]["path"], "api.py");
    }
    let normal = client.call("skel_list_files", json!({}));
    assert_eq!(success(&normal)["file_count"], 1);
    let explicit = client.call("skel_skeletonize", json!({"path": "target"}));
    assert_eq!(success(&explicit)["files"][0]["path"], "internal.py");
    assert!(markdown(&explicit).contains("def internal():\n    ..."));
    client.close();
}

#[test]
fn context_metadata_and_statistics_match_every_file_and_mode_exactly() {
    let fixture = Fixture::new();
    let cases = [
        (
            "a.py",
            "python",
            "FULL",
            "def full():\n    return '中文 🐾'\n",
            "def full():\n    return '中文 🐾'\n",
        ),
        (
            "b.ts",
            "typescript",
            "SKELETON",
            "function pruned() { return 123456789; }\n",
            "function pruned() { /* ... */ }\n",
        ),
        (
            "c.py",
            "python",
            "UNCHANGED",
            "def broken(\n",
            "def broken(\n",
        ),
        (
            "d.py",
            "python",
            "UNCHANGED",
            "CONSTANT = 1\n",
            "CONSTANT = 1\n",
        ),
        ("e.py", "python", "UNCHANGED", "", ""),
    ];
    for (path, _, _, source, _) in cases {
        fixture.write(path, source);
    }
    let before = fixture.snapshot();
    let mut client = Client::new(&fixture);
    let response = client.call("skel_skeletonize", json!({"focus": ["a.py"]}));
    let data = success(&response);
    let results = cases.map(|(path, language, mode, source, content)| skel::FileResult {
        path: path.into(),
        lang: language.into(),
        mode: mode.into(),
        orig_tokens: skel::estimate_tokens(source),
        skel_tokens: skel::estimate_tokens(content),
        content: content.into(),
    });
    let entries = results.iter().map(|file| json!({"path": file.path, "language": file.lang, "mode": file.mode, "original_estimated_tokens": file.orig_tokens, "skeleton_estimated_tokens": file.skel_tokens})).collect::<Vec<_>>();
    assert_eq!(data["files"], json!(entries));
    let stats = skel::compute_stats(&results);
    assert_eq!(
        data["stats"],
        json!({
            "file_count": 5, "original_estimated_tokens": stats.orig_tokens,
            "skeleton_estimated_tokens": stats.skel_tokens, "saved_estimated_tokens": stats.saved_tokens,
            "reduction_percent": stats.ratio, "token_estimation": "UTF-8 bytes / 3.5; excludes Markdown overhead"
        })
    );
    assert_eq!(markdown(&response), skel::format_markdown(&results, &stats));
    assert_eq!(data["warnings"], json!([]));
    assert_eq!(
        data["markdown_estimated_tokens"],
        skel::estimate_tokens(markdown(&response))
    );
    assert_eq!(fixture.snapshot(), before);
    client.close();
}

#[test]
fn focus_overrides_unchanged_mode_even_for_invalid_and_empty_source() {
    let fixture = Fixture::new();
    fixture.write("broken.py", "def broken(\n");
    fixture.write("empty.py", "");
    let mut client = Client::new(&fixture);
    let response = client.call("skel_skeletonize", json!({"focus": ["*.py"]}));
    let data = success(&response);
    for file in data["files"].as_array().unwrap() {
        assert_eq!(file["mode"], "FULL");
        assert_eq!(
            file["original_estimated_tokens"],
            file["skeleton_estimated_tokens"]
        );
    }
    assert_eq!(data["stats"]["saved_estimated_tokens"], 0);
    assert_eq!(data["stats"]["reduction_percent"], 0.0);
    client.close();
}

#[test]
fn empty_and_unsupported_inputs_return_complete_zero_statistics() {
    let fixture = Fixture::new();
    fixture.write("notes.md", "# unsupported");
    let mut client = Client::new(&fixture);
    for arguments in [json!({}), json!({"path": "notes.md"})] {
        let listing = client.call("skel_list_files", arguments.clone());
        assert_eq!(
            success(&listing),
            &json!({"file_count": 0, "files": [], "warnings": []})
        );
        let context = client.call("skel_skeletonize", arguments);
        let data = success(&context);
        assert_eq!(data["files"], json!([]));
        assert_eq!(data["warnings"], json!([]));
        for field in [
            "file_count",
            "original_estimated_tokens",
            "skeleton_estimated_tokens",
            "saved_estimated_tokens",
        ] {
            assert_eq!(data["stats"][field], 0);
        }
        assert_eq!(data["stats"]["reduction_percent"], 0.0);
        assert!(!markdown(&context).contains("### File:"));
    }
    client.close();
}

#[test]
fn listing_does_not_read_or_reject_large_or_non_utf8_source_contents() {
    let fixture = Fixture::new();
    fixture.write_bytes("binary.py", &[0xff, 0xfe]);
    fixture.write("huge.py", &"x".repeat(2 * 1024 * 1024 + 1));
    let mut client = Client::new(&fixture);
    let listing = client.call("skel_list_files", json!({}));
    assert_eq!(success(&listing)["file_count"], 2);
    assert_eq!(success(&listing)["warnings"], json!([]));
    tool_error(
        &client.call("skel_skeletonize", json!({"path": "huge.py"})),
        "exceeds 2 MiB",
    );
    tool_error(
        &client.call("skel_skeletonize", json!({"path": "binary.py"})),
        "not UTF-8",
    );
    success(&client.call("skel_list_files", json!({})));
    client.close();
}

#[test]
fn unreadable_file_warnings_identify_paths_and_focus_does_not_suppress_them() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 123\n");
    fixture.write_bytes("binary.py", &[0xff]);
    let before = fixture.snapshot();
    let mut client = Client::new(&fixture);
    let response = client.call("skel_skeletonize", json!({"focus": ["binary.py"]}));
    let data = success(&response);
    assert_eq!(data["stats"]["file_count"], 1);
    assert_eq!(data["files"].as_array().unwrap().len(), 1);
    assert_eq!(data["files"][0]["path"], "api.py");
    assert_eq!(
        data["warnings"],
        json!(["Skipped 'binary.py': source is not UTF-8"])
    );
    assert!(!markdown(&response).contains("binary.py"));
    assert_eq!(fixture.snapshot(), before);
    client.close();
}

fn prunable_source_of_size(bytes: usize) -> String {
    let prefix = "def f():\n    value = \"";
    let suffix = "\"\n    return value\n";
    let source = format!(
        "{prefix}{}{suffix}",
        "x".repeat(bytes - prefix.len() - suffix.len())
    );
    assert_eq!(source.len(), bytes);
    source
}

#[test]
fn source_byte_limit_accepts_below_and_exact_boundary_and_rejects_one_extra_byte() {
    let fixture = Fixture::new();
    let limit = 2 * 1024 * 1024;
    let mut client = Client::new(&fixture);
    for bytes in [limit - 1, limit] {
        fixture.write("large.py", &prunable_source_of_size(bytes));
        let response = client.call("skel_skeletonize", json!({"path": "large.py"}));
        assert_eq!(success(&response)["files"][0]["mode"], "SKELETON");
        assert!(markdown(&response).contains("def f():\n    ...\n"));
    }
    fixture.write("large.py", &prunable_source_of_size(limit + 1));
    for focus in [json!([]), json!(["large.py"])] {
        tool_error(
            &client.call(
                "skel_skeletonize",
                json!({"path": "large.py", "focus": focus}),
            ),
            "exceeds 2 MiB",
        );
    }
    assert!(client.request("ping", json!({}))["result"].is_object());
    client.close();
}

#[test]
fn source_size_limits_count_utf8_bytes_instead_of_unicode_characters() {
    let fixture = Fixture::new();
    let source = format!(
        "def f():\n    value = \"{}\"\n    return value\n",
        "中".repeat(700_000)
    );
    assert!(source.chars().count() < 2 * 1024 * 1024);
    assert!(source.len() > 2 * 1024 * 1024);
    fixture.write("unicode.py", &source);
    let mut client = Client::new(&fixture);
    for focus in [json!([]), json!(["unicode.py"])] {
        tool_error(
            &client.call(
                "skel_skeletonize",
                json!({"path": "unicode.py", "focus": focus}),
            ),
            "exceeds 2 MiB",
        );
    }
    assert_eq!(
        success(&client.call("skel_list_files", json!({})))["file_count"],
        1
    );
    client.close();
}

#[test]
fn aggregate_source_limit_accepts_exactly_sixteen_mib_and_rejects_one_extra_byte() {
    let fixture = Fixture::new();
    let source = prunable_source_of_size(2 * 1024 * 1024);
    for index in 0..8 {
        fixture.write(&format!("large/{index}.py"), &source);
    }
    let mut client = Client::new(&fixture);
    let exact = client.call("skel_skeletonize", json!({"path": "large"}));
    let data = success(&exact);
    assert_eq!(data["stats"]["file_count"], 8);
    assert_eq!(
        data["stats"]["original_estimated_tokens"],
        8 * skel::estimate_tokens(&source)
    );
    assert_eq!(markdown(&exact).matches("def f():").count(), 8);
    fixture.write("large/extra.py", "\n");
    tool_error(
        &client.call("skel_skeletonize", json!({"path": "large"})),
        "exceeds 16 MiB",
    );
    assert_eq!(
        success(&client.call("skel_list_files", json!({"path": "large"})))["file_count"],
        9
    );
    fixture.write("api.py", "def api():\n    return 1\n");
    success(&client.call("skel_skeletonize", json!({"path": "api.py"})));
    client.close();
}

#[test]
fn response_limit_counts_json_escaping_and_returns_no_partial_context() {
    let fixture = Fixture::new();
    let mut client = Client::new(&fixture);
    fixture.write("lines.py", &"\n".repeat(900_000));
    let below = client.call(
        "skel_skeletonize",
        json!({"path": "lines.py", "focus": ["lines.py"]}),
    );
    assert!(markdown(&below).len() < 2 * 1024 * 1024);
    assert!(serde_json::to_vec(&below["result"]).unwrap().len() < 2 * 1024 * 1024);
    fixture.write("lines.py", &"\n".repeat(1_100_000));
    // Source and raw Markdown fit, but escaping every newline exceeds the response budget.
    tool_error(
        &client.call(
            "skel_skeletonize",
            json!({"path": "lines.py", "focus": ["lines.py"]}),
        ),
        "Response exceeds 2 MiB",
    );
    success(&client.call("skel_list_files", json!({})));
    assert!(client.request("ping", json!({}))["result"].is_object());
    client.close();
}

#[test]
fn pipelined_requests_keep_response_ids_and_request_specific_focus_separate() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 123\n");
    let before = fixture.snapshot();
    let mut client = Client::new(&fixture);
    let mut ids = Vec::new();
    for index in 0..12 {
        let focus = if index % 2 == 0 {
            json!(["api.py"])
        } else {
            json!([])
        };
        ids.push(client.send_request(
            "tools/call",
            json!({"name": "skel_skeletonize", "arguments": {"focus": focus}}),
        ));
    }
    // Reverse consumption also exercises the client's out-of-order response buffer.
    for (index, id) in ids.into_iter().enumerate().rev() {
        let response = client.receive(id);
        let expected = if index % 2 == 0 { "FULL" } else { "SKELETON" };
        assert_eq!(success(&response)["files"][0]["mode"], expected);
        assert_eq!(markdown(&response).contains("return 123"), index % 2 == 0);
    }
    assert_eq!(fixture.snapshot(), before);
    client.close();
}

#[test]
fn repeat_requests_are_deterministic_and_source_changes_are_read_fresh() {
    let fixture = Fixture::new();
    fixture.write("api.py", "def api():\n    return 123\n");
    let mut client = Client::new(&fixture);
    let first = client.call("skel_skeletonize", json!({}));
    let repeated = client.call("skel_skeletonize", json!({}));
    assert_eq!(first["result"], repeated["result"]);
    fixture.write("api.py", "def changed():\n    return 456\n");
    let changed = client.call("skel_skeletonize", json!({}));
    assert!(markdown(&changed).contains("def changed():"));
    assert!(!markdown(&changed).contains("def api():"));
    fs::remove_file(fixture.0.join("api.py")).unwrap();
    assert_eq!(
        success(&client.call("skel_list_files", json!({})))["file_count"],
        0
    );
    client.close();
}

#[test]
fn sibling_directories_with_the_same_name_prefix_cannot_bypass_root_containment() {
    let fixture = Fixture::new();
    fixture.write("project/api.py", "def api():\n    return 1\n");
    fixture.write(
        "project-backup/private.py",
        "SENSITIVE_SOURCE_MARKER = 123\n",
    );
    let before = fixture.snapshot();
    let mut client = Client::with_root(&fixture.0.join("project"));
    for tool in ["skel_list_files", "skel_skeletonize"] {
        for path in [
            "../project-backup".into(),
            "../project-backup/private.py".into(),
            fixture
                .0
                .join("project-backup")
                .to_string_lossy()
                .into_owned(),
            fixture
                .0
                .join("project-backup/private.py")
                .to_string_lossy()
                .into_owned(),
        ] {
            let response = client.call(tool, json!({"path": path}));
            let message = tool_error(&response, "outside");
            assert!(!message.contains("SENSITIVE_SOURCE_MARKER"));
        }
        success(&client.call(tool, json!({})));
    }
    assert_eq!(fixture.snapshot(), before);
    client.close();
}

#[test]
fn uppercase_tsx_and_jsx_extensions_select_the_jsx_parser() {
    let fixture = Fixture::new();
    fixture.write(
        "View.TSX",
        "export const View = (props: { x: number }) => <div>{props.x}</div>;\n",
    );
    fixture.write(
        "Other.JSX",
        "export const Other = () => <div>secret</div>;\n",
    );
    let mut client = Client::new(&fixture);
    let response = client.call("skel_skeletonize", json!({}));
    let files = success(&response)["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["language"], "javascript");
    assert_eq!(files[1]["language"], "typescript");
    for file in files {
        assert_eq!(file["mode"], "SKELETON");
    }
    assert!(!markdown(&response).contains("<div>"));
    assert!(
        markdown(&response).contains("props: { x: number }")
            && markdown(&response).contains("=> { /* ... */ }")
    );
    client.close();
}

#[cfg(unix)]
#[test]
fn internal_symlinks_are_allowed_and_directory_links_cannot_escape_the_root() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    fixture.write("src/api.py", "def api():\n    return 1\n");
    outside.write("secret.py", "def secret():\n    return 2\n");
    std::os::unix::fs::symlink("src/api.py", fixture.0.join("internal.py")).unwrap();
    std::os::unix::fs::symlink(&outside.0, fixture.0.join("external_directory")).unwrap();
    let mut client = Client::new(&fixture);
    let response = client.call("skel_skeletonize", json!({}));
    assert_eq!(success(&response)["stats"]["file_count"], 2);
    assert!(markdown(&response).contains("`internal.py`"));
    for tool in ["skel_list_files", "skel_skeletonize"] {
        tool_error(
            &client.call(tool, json!({"path": "external_directory/secret.py"})),
            "outside",
        );
    }
    client.close();
}
