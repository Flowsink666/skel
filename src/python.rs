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
    collect_py_prune_ranges(tree.root_node(), source, &mut ranges);

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

fn collect_py_prune_ranges(node: Node, source: &str, ranges: &mut Vec<(usize, usize, String)>) {
    let kind = node.kind();
    if kind == "function_definition" || kind == "async_function_definition" {
        if let Some(block) = node.child_by_field_name("body") {
            let mut cursor = block.walk();
            let children: Vec<Node> = block.children(&mut cursor).collect();

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
                if child.kind() == "expression_statement" {
                    if let Some(str_child) = child.child(0) {
                        if str_child.kind() == "string" {
                            docstring_node = Some(*child);
                            break;
                        }
                    }
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
            return; // Do not recurse into pruned function body
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_py_prune_ranges(child, source, ranges);
    }
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
