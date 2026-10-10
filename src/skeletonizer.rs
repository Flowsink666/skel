use crate::c_like::skeletonize_c_like;
use crate::python::skeletonize_python;
use crate::typescript::skeletonize_typescript;
use tree_sitter::Node;

/// Walk an AST without using the call stack. Returning false skips a subtree.
pub(crate) fn walk_prunable(root: Node, mut visit: impl FnMut(Node) -> bool) {
    let mut cursor = root.walk();
    loop {
        if visit(cursor.node()) && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

/// Route source code to language-specific skeletonizer.
pub fn skeletonize(code: &str, lang: &str, is_jsx: bool) -> String {
    match lang {
        "python" => skeletonize_python(code),
        "typescript" => skeletonize_typescript(code, is_jsx),
        "javascript" => skeletonize_typescript(code, is_jsx),
        "go" | "rust" | "java" | "c" | "cpp" => skeletonize_c_like(code),
        _ => code.to_string(),
    }
}
