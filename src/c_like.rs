use regex::Regex;
use std::sync::LazyLock;

static CONTROL_FLOW_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(if|for|while|switch|catch|with|else|finally|do|match)\b").unwrap()
});
static TYPE_DECL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(pub(?:\([^)]*\))?\s+|export\s+|public\s+|private\s+|protected\s+)*(type|interface|enum|struct|trait|impl|class|union|namespace)\b").unwrap()
});
static FUNCTION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(fn|func)\s+|\bfunc\s*\(").unwrap());

/// Conservative lexical fallback for C-family languages. Strings and comments
/// are masked before matching braces; malformed/unbalanced input is kept intact.
pub fn skeletonize_c_like(code: &str) -> String {
    let Some(mask) = mask_literals(code) else {
        return code.to_string();
    };
    let mut stack = Vec::new();
    let mut ranges = Vec::new();
    let mut boundary = 0;
    for (i, &byte) in mask.iter().enumerate() {
        match byte {
            b'{' => {
                let prefix = String::from_utf8_lossy(&mask[boundary..i]);
                stack.push((i, is_function_signature(&prefix)));
                boundary = i + 1;
            }
            b'}' => {
                let Some((start, is_function)) = stack.pop() else {
                    return code.to_string();
                };
                if is_function {
                    ranges.push((start, i + 1));
                }
                boundary = i + 1;
            }
            b';' => boundary = i + 1,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return code.to_string();
    }
    ranges.sort_unstable();
    let mut output = String::with_capacity(code.len());
    let mut offset = 0;
    for (start, end) in ranges {
        if start < offset {
            continue;
        }
        output.push_str(&code[offset..start]);
        output.push_str("{ /* ... */ }");
        offset = end;
    }
    output.push_str(&code[offset..]);
    output
}

fn is_function_signature(prefix: &str) -> bool {
    let prefix = prefix.trim();
    if TYPE_DECL_PATTERN.is_match(prefix) || CONTROL_FLOW_PATTERN.is_match(prefix) {
        return false;
    }
    if FUNCTION_PATTERN.is_match(prefix) {
        return true;
    }
    // Avoid interpreting initializer expressions and macros as declarations.
    prefix.contains('(')
        && prefix.contains(')')
        && !prefix.contains('=')
        && !prefix.contains('!')
        && !prefix.starts_with('#')
        && !prefix.starts_with("return ")
}

fn blank(mask: &mut [u8], start: usize, end: usize) {
    for byte in &mut mask[start..end] {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

/// Produce a byte-aligned mask, retaining only structural source tokens.
fn mask_literals(code: &str) -> Option<Vec<u8>> {
    let bytes = code.as_bytes();
    let mut mask = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        if bytes[i..].starts_with(b"//") {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth != 0 {
                return None;
            }
        } else if (bytes[i] == b'r' || bytes[i..].starts_with(b"br"))
            && (i == 0 || !is_identifier_byte(bytes[i - 1]))
        {
            // Rust raw strings, including r###"..."###.
            let hashes = i + if bytes[i] == b'b' { 2 } else { 1 };
            let mut quote = hashes;
            while quote < bytes.len() && bytes[quote] == b'#' {
                quote += 1;
            }
            if quote >= bytes.len() || bytes[quote] != b'"' {
                i += 1;
                continue;
            }
            let mut terminator = vec![b'"'];
            terminator.extend_from_slice(&bytes[hashes..quote]);
            i = find_bytes(bytes, quote + 1, &terminator)? + terminator.len();
        } else if bytes[i..].starts_with(b"R\"") {
            // C++ raw strings: R"delimiter(contents)delimiter".
            let opening = bytes[i + 2..].iter().position(|&b| b == b'(')? + i + 2;
            let mut terminator = vec![b')'];
            terminator.extend_from_slice(&bytes[i + 2..opening]);
            terminator.push(b'"');
            i = find_bytes(bytes, opening + 1, &terminator)? + terminator.len();
        } else if bytes[i] == b'"' || bytes[i] == b'`' {
            let quote = bytes[i];
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\\' && quote != b'`' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            if !closed {
                return None;
            }
        } else if bytes[i] == b'\'' {
            // Rust lifetimes are not quoted strings. Match character literals
            // only when a closing quote follows a character or escape sequence.
            let tail = &code[i + 1..];
            let char_len = tail.chars().next().map(char::len_utf8).unwrap_or(0);
            let mut end = i + 1 + char_len;
            if bytes.get(i + 1) == Some(&b'\\') {
                end = i + 2;
                while end < bytes.len() && bytes[end] != b'\'' && bytes[end] != b'\n' {
                    end += 1;
                }
            }
            if bytes.get(end) == Some(&b'\'') {
                i = end + 1;
            } else {
                i += 1;
                continue;
            }
        } else {
            i += 1;
            continue;
        }
        blank(&mut mask, start, i);
    }
    Some(mask)
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn find_bytes(bytes: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    bytes[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|i| start + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_following_declarations_with_braces_in_literals_and_comments() {
        for literal in [
            "\"{\"",
            "\"}\"",
            "r###\"{\"inside}\"###",
            "br###\"{\"inside}\"###",
            "'{'",
            "`{`",
            "R\"tag({\"})tag\"",
        ] {
            let code = format!("pub fn first() {{\n let _ = {literal}; /* {{ }} */ // }}\n}}\npub fn second() {{ let _ = 2; }}\n");
            let output = skeletonize_c_like(&code);
            assert_eq!(
                output, "pub fn first() { /* ... */ }\npub fn second() { /* ... */ }\n",
                "{code}"
            );
        }
    }

    #[test]
    fn handles_nested_blocks_and_multiline_signatures() {
        let code =
            "pub fn first(\n x: i32,\n) -> i32 { if x > 0 { x } else { 0 } }\npub fn next() {}\n";
        assert_eq!(
            skeletonize_c_like(code),
            "pub fn first(\n x: i32,\n) -> i32 { /* ... */ }\npub fn next() { /* ... */ }\n"
        );
    }

    #[test]
    fn retains_types_lifetimes_and_suffixes() {
        let code = "struct S { value: i32 }\nimpl S {\n pub fn value<'a>(&'a self) -> &'a i32 { &self.value }\n}\n";
        assert_eq!(skeletonize_c_like(code), "struct S { value: i32 }\nimpl S {\n pub fn value<'a>(&'a self) -> &'a i32 { /* ... */ }\n}\n");
        let java = "class A { public int get() { return 1; } }\n";
        assert_eq!(
            skeletonize_c_like(java),
            "class A { public int get() { /* ... */ } }\n"
        );
    }

    #[test]
    fn keeps_unbalanced_input_and_initializers_intact() {
        for code in [
            "pub fn broken() {",
            "pub fn broken() { let _ = \"unterminated; }",
            "const x = make() { 1 };",
            "struct S { int x; };",
        ] {
            assert_eq!(skeletonize_c_like(code), code);
        }
    }
}
