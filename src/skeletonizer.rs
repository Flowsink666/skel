use crate::c_like::skeletonize_c_like;
use crate::python::skeletonize_python;
use crate::typescript::skeletonize_typescript;

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
