use tree_sitter::{Node, Parser};

/// Prune implementation bodies in TypeScript/JavaScript AST while retaining
/// classes, interfaces, types, signatures, and JSDoc docstrings.
pub fn skeletonize_typescript(source: &str, is_tsx: bool) -> String {
    let mut parser = Parser::new();
    let lang = if is_tsx {
        tree_sitter_typescript::LANGUAGE_TSX
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT
    };

    if parser.set_language(&lang.into()).is_err() {
        return source.to_string();
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return source.to_string(),
    };
    if tree.root_node().has_error() {
        return source.to_string();
    }

    let mut ranges = Vec::new();
    collect_ts_prune_ranges(tree.root_node(), &mut ranges);

    ranges.sort_by_key(|&(start, _, _)| start);

    let mut result = String::with_capacity(source.len());
    let mut last_offset = 0;

    for (start, end, replacement) in ranges {
        if start < last_offset {
            continue;
        }
        result.push_str(&source[last_offset..start]);
        result.push_str(replacement);
        last_offset = end;
    }
    result.push_str(&source[last_offset..]);
    result
}

fn collect_ts_prune_ranges(node: Node, ranges: &mut Vec<(usize, usize, &'static str)>) {
    let kind = node.kind();
    match kind {
        "function_declaration"
        | "method_definition"
        | "generator_function_declaration"
        | "function_expression" => {
            if let Some(body) = node.child_by_field_name("body") {
                if body.kind() == "statement_block" {
                    ranges.push((body.start_byte(), body.end_byte(), "{ /* ... */ }"));
                    return; // Do not recurse into pruned body
                }
            }
        }
        "arrow_function" => {
            if let Some(body) = node.child_by_field_name("body") {
                ranges.push((body.start_byte(), body.end_byte(), "{ /* ... */ }"));
                return;
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_prune_ranges(child, ranges);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prunes_expression_arrows_and_preserves_following_declarations() {
        for (code, tsx) in [
            (
                "export const f = (x: number): number => x * 2;\nexport const next = 1;",
                false,
            ),
            (
                "const f = () => ({ secret: expensive() });\nconst next = 1;",
                false,
            ),
            (
                "export const View = () => <div>{expensive()}</div>;\nconst next = 1;",
                true,
            ),
        ] {
            let output = skeletonize_typescript(code, tsx);
            let mut parser = Parser::new();
            let lang = if tsx {
                tree_sitter_typescript::LANGUAGE_TSX
            } else {
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT
            };
            parser.set_language(&lang.into()).unwrap();
            assert!(
                !parser.parse(&output, None).unwrap().root_node().has_error(),
                "{output}"
            );
            assert!(output.contains("next = 1"));
            assert!(output.contains("=> { /* ... */ }"));
            assert!(!output.contains("x * 2"));
            assert!(!output.contains("expensive()"));
        }
    }

    #[test]
    fn test_ts_async_method_skeletonize() {
        let code = r#"
export class ApiClient {
    private token: string;

    /**
     * Fetch user profile
     */
    public async getUser(userId: string): Promise<ApiResponse<UserProfile>> {
        const url = `${this.baseUrl}/users/${userId}`;
        if (this.token) {
            console.log("Token present");
        }
        return await fetch(url);
    }
}
"#;
        let pruned = skeletonize_typescript(code, false);
        assert!(pruned.contains("export class ApiClient"));
        assert!(pruned.contains(
            "public async getUser(userId: string): Promise<ApiResponse<UserProfile>> { /* ... */ }"
        ));
        assert!(pruned.contains("/**\n     * Fetch user profile\n     */"));
        assert!(!pruned.contains("console.log"));
        assert!(!pruned.contains("const url"));
    }

    #[test]
    fn test_ts_arrow_function() {
        let code = r#"
export const formatName = (first: string, last: string): string => {
    const cleanFirst = first.trim();
    const cleanLast = last.trim();
    return `${cleanFirst} ${cleanLast}`;
};
"#;
        let pruned = skeletonize_typescript(code, false);
        assert!(pruned.contains(
            "export const formatName = (first: string, last: string): string => { /* ... */ };"
        ));
        assert!(!pruned.contains("cleanFirst"));
    }
}
