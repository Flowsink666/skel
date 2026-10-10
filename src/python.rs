use crate::skeletonizer::walk_prunable;
use tree_sitter::{Node, Parser};

/// Prune implementation bodies in Python AST while retaining
/// class declarations, method signatures, decorators, type hints, and docstrings.
pub fn skeletonize_python(source: &str) -> String {
    let mut parser = Parser::new();
    let lang = tree_sitter_python::LANGUAGE;
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
    if !collect_py_prune_ranges(tree.root_node(), source, &mut ranges) {
        return source.to_string();
    }

    ranges.sort_by_key(|&(start, _, _)| start);

    let mut result = String::with_capacity(source.len());
    let mut last_offset = 0;

    for (start, end, replacement) in ranges {
        if start < last_offset {
            continue;
        }
        result.push_str(&source[last_offset..start]);
        result.push_str(&replacement);
        last_offset = end;
    }
    result.push_str(&source[last_offset..]);
    result
}

fn is_valid_docstring_stmt(stmt: Node, source: &str) -> bool {
    if stmt.kind() != "expression_statement" {
        return false;
    }
    let mut expr = match stmt.child(0) {
        Some(c) => c,
        None => return false,
    };
    while expr.kind() == "parenthesized_expression" {
        if let Some(inner) = expr.named_child(0) {
            expr = inner;
        } else {
            return false;
        }
    }
    is_valid_docstring_expr(expr, source)
}

fn is_valid_docstring_expr(expr: Node, source: &str) -> bool {
    match expr.kind() {
        "string" => is_valid_docstring_string_literal(expr, source),
        "concatenated_string" => {
            let mut cursor = expr.walk();
            let mut count = 0;
            for child in expr.children(&mut cursor) {
                if child.kind() == "comment" {
                    continue;
                }
                if child.kind() == "string" {
                    if !is_valid_docstring_string_literal(child, source) {
                        return false;
                    }
                    count += 1;
                } else {
                    return false;
                }
            }
            count > 0
        }
        _ => false,
    }
}

fn is_valid_docstring_string_literal(node: Node, source: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "interpolation" {
            return false;
        }
        if child.kind() == "string_start" {
            let text = &source[child.byte_range()];
            let prefix: String = text.chars().take_while(|c| c.is_alphabetic()).collect();
            let prefix_lower = prefix.to_ascii_lowercase();
            if prefix_lower.contains('b') || prefix_lower.contains('f') {
                return false;
            }
        }
    }
    true
}

fn collect_py_prune_ranges(
    node: Node,
    source: &str,
    ranges: &mut Vec<(usize, usize, String)>,
) -> bool {
    let mut valid = true;
    walk_prunable(node, |node| {
        let kind = node.kind();
        if kind == "function_definition" || kind == "async_function_definition" {
            let block = match node.child_by_field_name("body") {
                Some(b) => b,
                None => {
                    valid = false;
                    return false;
                }
            };
            let mut cursor = block.walk();
            let children: Vec<Node> = block.children(&mut cursor).collect();
            if children.is_empty() || children.iter().all(|c| c.kind() == "comment") {
                valid = false;
                return false;
            }

            // Preserve actual tabs/spaces; a byte column is not an indentation width.
            let line_start = source[..block.start_byte()]
                .rfind('\n')
                .map_or(0, |i| i + 1);
            let prefix = &source[line_start..block.start_byte()];
            let inline = !prefix.chars().all(|c| c == ' ' || c == '\t' || c == '\r');

            let mut docstring_node = None;
            for child in &children {
                if child.kind() == "comment" {
                    continue;
                }
                if is_valid_docstring_stmt(*child, source) {
                    docstring_node = Some(*child);
                }
                break;
            }

            if let Some(doc_node) = docstring_node {
                let prune_start = doc_node.end_byte();
                let prune_end = block.end_byte();
                if prune_start < prune_end {
                    let replacement = if inline {
                        "; ...".to_string()
                    } else {
                        format!("\n{prefix}...")
                    };
                    ranges.push((prune_start, prune_end, replacement));
                }
            } else {
                let prune_start = block.start_byte();
                let prune_end = block.end_byte();
                let replacement = "...".to_string();
                ranges.push((prune_start, prune_end, replacement));
            }
            return false; // Do not descend into a pruned function body.
        }

        if kind == "class_definition" {
            let block = match node.child_by_field_name("body") {
                Some(b) => b,
                None => {
                    valid = false;
                    return false;
                }
            };
            let mut cursor = block.walk();
            let children: Vec<Node> = block.children(&mut cursor).collect();
            if children.is_empty() || children.iter().all(|c| c.kind() == "comment") {
                valid = false;
                return false;
            }
        }

        true
    });

    valid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_docstrings_with_inline_and_tab_indentation() {
        for code in [
            "def f(): \"\"\"doc\"\"\"; return 1\n",
            "def f():\n\t\"\"\"doc\"\"\"\n\treturn 1\n",
            "class C:\n\tdef f(self):\n\t\t\"\"\"doc\"\"\"\n\t\treturn 1\n",
        ] {
            let pruned = skeletonize_python(code);
            let mut parser = Parser::new();
            parser
                .set_language(&tree_sitter_python::LANGUAGE.into())
                .unwrap();
            assert!(
                !parser.parse(&pruned, None).unwrap().root_node().has_error(),
                "{pruned}"
            );
            assert!(pruned.contains("\"\"\"doc\"\"\""));
            assert!(!pruned.contains("return 1"));
            if code.contains('\t') {
                assert!(pruned.contains("\n\t"));
            } else {
                assert!(pruned.contains("; ..."));
            }
        }
    }

    #[test]
    fn test_python_skeletonize_simple() {
        let code = r#"
class Foo:
    def bar(self, x: int) -> int:
        """Double x."""
        y = x * 2
        return y
"#;
        let pruned = skeletonize_python(code);
        assert!(pruned.contains("class Foo:"));
        assert!(pruned.contains("def bar(self, x: int) -> int:"));
        assert!(pruned.contains("\"\"\"Double x.\"\"\""));
        assert!(pruned.contains("..."));
        assert!(!pruned.contains("y = x * 2"));
    }

    #[test]
    fn test_python_skeletonize_no_docstring() {
        let code = r#"
def hello(name: str):
    print("hello " + name)
    return True
"#;
        let pruned = skeletonize_python(code);
        assert!(pruned.contains("def hello(name: str):"));
        assert!(pruned.contains("..."));
        assert!(!pruned.contains("print"));
    }
}
