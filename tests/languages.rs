use skel::skeletonize;
use tree_sitter::Parser;

fn assert_parses(source: &str, language: &str, jsx: bool) {
    assert!(
        !has_parse_error(source, language, jsx),
        "Invalid {language} syntax:\n{source}"
    );
}

fn has_parse_error(source: &str, language: &str, jsx: bool) -> bool {
    let mut parser = Parser::new();
    let grammar = match (language, jsx) {
        ("python", _) => tree_sitter_python::LANGUAGE.into(),
        (_, true) => tree_sitter_typescript::LANGUAGE_TSX.into(),
        _ => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    };
    parser.set_language(&grammar).unwrap();
    let tree = parser.parse(source, None).unwrap();
    tree.root_node().has_error()
}

fn golden(language: &str, jsx: bool, source: &str, expected: &str) {
    if matches!(language, "python" | "typescript" | "javascript") {
        assert_parses(source, language, jsx);
        assert_parses(expected, language, jsx);
    }
    let output = skeletonize(source, language, jsx);
    assert_eq!(output, expected, "Source:\n{source}");
    assert_eq!(
        skeletonize(&output, language, jsx),
        output,
        "Skeletonization must be idempotent"
    );
}

macro_rules! golden_test {
    ($name:ident, $language:literal, $jsx:literal, $source:expr, $expected:expr) => {
        #[test]
        fn $name() {
            golden($language, $jsx, $source, $expected);
        }
    };
}

golden_test!(python_multiline_async_signatures_decorators_and_defaults, "python", false,
    "from typing import Any\n\n@cache(ttl=30)\nasync def fetch(\n    key: str, /,\n    options: dict[str, Any] = {\"retry\": 3},\n    *, timeout: float = 1.5,\n) -> list[str]:\n    \"\"\"Fetch a value.\"\"\"\n    await expensive(key)\n    return [key]\n\nVERSION = 2\n",
    "from typing import Any\n\n@cache(ttl=30)\nasync def fetch(\n    key: str, /,\n    options: dict[str, Any] = {\"retry\": 3},\n    *, timeout: float = 1.5,\n) -> list[str]:\n    \"\"\"Fetch a value.\"\"\"\n    ...\n\nVERSION = 2\n");

golden_test!(python_class_structure_properties_and_docstrings, "python", false,
    "\"\"\"Module documentation.\"\"\"\nclass Service(Base):\n    \"\"\"Service documentation.\"\"\"\n    limit: int = 3\n\n    @property\n    def name(self) -> str:\n        \"\"\"Public name.\"\"\"\n        return self._name\n\n    @name.setter\n    def name(self, value: str):\n        self._name = value\n",
    "\"\"\"Module documentation.\"\"\"\nclass Service(Base):\n    \"\"\"Service documentation.\"\"\"\n    limit: int = 3\n\n    @property\n    def name(self) -> str:\n        \"\"\"Public name.\"\"\"\n        ...\n\n    @name.setter\n    def name(self, value: str):\n        ...\n");

golden_test!(python_outer_function_removes_all_nested_implementations, "python", false,
    "def outer(x):\n    def inner():\n        return 'secret'\n    class Hidden:\n        def method(self):\n            return 123\n    return inner()\n\ndef sibling():\n    return 456\n",
    "def outer(x):\n    ...\n\ndef sibling():\n    ...\n");

golden_test!(python_functions_inside_top_level_control_flow, "python", false,
    "if ENABLED:\n    def active():\n        return 1\nelse:\n    def inactive():\n        return 2\n",
    "if ENABLED:\n    def active():\n        ...\nelse:\n    def inactive():\n        ...\n");

golden_test!(python_multiline_docstring_keeps_examples_and_indentation, "python", false,
    "def documented():\n    '''説明。\n\n    >>> documented()\n    {'ok': True}\n    '''\n    return {'secret': True}\n",
    "def documented():\n    '''説明。\n\n    >>> documented()\n    {'ok': True}\n    '''\n    ...\n");

golden_test!(
    python_inline_docstring_and_semicolon_statements,
    "python",
    false,
    "def documented(): 'doc'; x = 1; return x\ndef plain(): x = 2; return x\n",
    "def documented(): 'doc'; ...\ndef plain(): ...\n"
);

golden_test!(
    python_tab_indented_class_method,
    "python",
    false,
    "class C:\n\tdef f(self):\n\t\t\"\"\"doc\"\"\"\n\t\treturn 1\n",
    "class C:\n\tdef f(self):\n\t\t\"\"\"doc\"\"\"\n\t\t...\n"
);

golden_test!(
    python_non_docstring_string_after_assignment_is_pruned,
    "python",
    false,
    "def f():\n    value = 1\n    'not a docstring'\n    return value\n",
    "def f():\n    ...\n"
);

golden_test!(
    python_docstring_only_function_is_unchanged,
    "python",
    false,
    "def f():\n    r'''only documentation'''\n",
    "def f():\n    r'''only documentation'''\n"
);

golden_test!(
    python_unicode_byte_offsets_and_no_final_newline,
    "python",
    false,
    "# 中文 🐾\ndef привет(名字: str = '世界'):\n    return 名字\n\n尾部 = '保留'",
    "# 中文 🐾\ndef привет(名字: str = '世界'):\n    ...\n\n尾部 = '保留'"
);

golden_test!(typescript_generics_overloads_and_type_declarations, "typescript", false,
    "import type { Item } from './item';\nexport interface Store<T> { get(key: string): T; }\nexport type Lookup<T> = (key: string) => T;\nexport function get(key: string): Item;\nexport function get(key: number): Item;\n/** Get an item. */\nexport function get<T extends Item>(key: string | number): T { return load(key); }\n",
    "import type { Item } from './item';\nexport interface Store<T> { get(key: string): T; }\nexport type Lookup<T> = (key: string) => T;\nexport function get(key: string): Item;\nexport function get(key: number): Item;\n/** Get an item. */\nexport function get<T extends Item>(key: string | number): T { /* ... */ }\n");

golden_test!(typescript_constructor_accessors_async_and_generator_methods, "typescript", false,
    "export class Service extends Base {\n  field: number = 7;\n  constructor(private token: string) { super(); initialize(); }\n  get value(): number { return this.field; }\n  set value(x: number) { this.field = x; }\n  async fetch(): Promise<number> { return await expensive(); }\n  *items(): Generator<number> { yield 99; }\n}\n",
    "export class Service extends Base {\n  field: number = 7;\n  constructor(private token: string) { /* ... */ }\n  get value(): number { /* ... */ }\n  set value(x: number) { /* ... */ }\n  async fetch(): Promise<number> { /* ... */ }\n  *items(): Generator<number> { /* ... */ }\n}\n");

golden_test!(typescript_arrows_in_fields_objects_and_arguments, "typescript", false,
    "class C { callback = (n: number) => n * 2; }\nconst object = { method(x: number) { return x; }, callback: () => ({ secret: 1 }) };\nconst mapped = values.map(value => value.secret);\nconst following = 7;\n",
    "class C { callback = (n: number) => { /* ... */ }; }\nconst object = { method(x: number) { /* ... */ }, callback: () => { /* ... */ } };\nconst mapped = values.map(value => { /* ... */ });\nconst following = 7;\n");

golden_test!(typescript_function_expression_and_nested_body, "typescript", false,
    "const f = function named(x: number) { function inner() { return 9; } return inner(); };\nexport default async function fetch() { return await expensive(); }\nexport function* values() { yield 1; }\n",
    "const f = function named(x: number) { /* ... */ };\nexport default async function fetch() { /* ... */ }\nexport function* values() { /* ... */ }\n");

golden_test!(typescript_declarations_and_top_level_literals_are_preserved, "typescript", false,
    "declare function api<T>(value: T): T;\nexport enum State { Ready, Busy }\nconst config = { braces: '{ }', pattern: /[{}]/, text: `中文 ${1}` };\n",
    "declare function api<T>(value: T): T;\nexport enum State { Ready, Busy }\nconst config = { braces: '{ }', pattern: /[{}]/, text: `中文 ${1}` };\n");

golden_test!(javascript_regex_template_literals_and_unicode, "javascript", false,
    "// 文档 🐾\nexport function 名称(x = /[{}]/) { const message = `brace } ${x}`; return message; }\nexport const 尾部 = '保留';",
    "// 文档 🐾\nexport function 名称(x = /[{}]/) { /* ... */ }\nexport const 尾部 = '保留';");

golden_test!(tsx_fragments_and_nested_jsx_callbacks, "typescript", true,
    "import React from 'react';\nexport const View = (props: { title: string }) => <><h1>{props.title}</h1><span>{items.map(x => <i>{x}</i>)}</span></>;\nexport const next = 1;\n",
    "import React from 'react';\nexport const View = (props: { title: string }) => { /* ... */ };\nexport const next = 1;\n");

golden_test!(jsx_function_components_and_top_level_markup, "javascript", true,
    "const staticElement = <div>keep</div>;\nexport default function App() { return <div>remove</div>; }\nconst View = () => <div>remove too</div>;\n",
    "const staticElement = <div>keep</div>;\nexport default function App() { /* ... */ }\nconst View = () => { /* ... */ };\n");

golden_test!(rust_types_impls_lifetimes_and_where_clause, "rust", false,
    "pub struct Store<T> { value: T }\nimpl<T> Store<T> {\n    pub fn get<'a>(&'a self) -> &'a T\n    where T: Clone\n    { &self.value }\n}\n",
    "pub struct Store<T> { value: T }\nimpl<T> Store<T> {\n    pub fn get<'a>(&'a self) -> &'a T\n    where T: Clone\n    { /* ... */ }\n}\n");

golden_test!(go_receivers_multiline_signatures_and_raw_literals, "go", false,
    "package api\ntype Store struct { Value int }\nfunc (s *Store) Read(\n key string,\n) (int, error) { text := `} {`; return s.Value, nil }\nfunc next() { println(1) }\n",
    "package api\ntype Store struct { Value int }\nfunc (s *Store) Read(\n key string,\n) (int, error) { /* ... */ }\nfunc next() { /* ... */ }\n");

golden_test!(java_class_constructor_and_method, "java", false,
    "class Api {\n private int value;\n public Api() { value = 1; }\n public int get() { if (value > 0) { return value; } return 0; }\n}\n",
    "class Api {\n private int value;\n public Api() { /* ... */ }\n public int get() { /* ... */ }\n}\n");

golden_test!(c_prototypes_preprocessor_and_functions, "c", false,
    "#include <stdio.h>\nint read_value(int x);\nstruct Value { int x; };\nint read_value(int x) { printf(\"}\"); return x; }\nint next(void) { return 2; }\n",
    "#include <stdio.h>\nint read_value(int x);\nstruct Value { int x; };\nint read_value(int x) { /* ... */ }\nint next(void) { /* ... */ }\n");

golden_test!(cpp_namespaces_classes_and_raw_strings, "cpp", false,
    "namespace api {\nclass Client {\npublic:\n int value() const { auto text = R\"tag({\"} /*)tag\"; return 7; }\n};\nint next() { return 8; }\n}\n",
    "namespace api {\nclass Client {\npublic:\n int value() const { /* ... */ }\n};\nint next() { /* ... */ }\n}\n");

#[test]
fn empty_and_whitespace_sources_round_trip_for_all_languages() {
    for language in [
        "python",
        "typescript",
        "javascript",
        "go",
        "rust",
        "java",
        "c",
        "cpp",
        "unknown",
    ] {
        for source in ["", " \t\r\n\n", "\n"] {
            assert_eq!(skeletonize(source, language, false), source, "{language}");
        }
    }
}

#[test]
fn python_comments_before_docstrings_and_signature_defaults_are_preserved() {
    golden("python", false,
        "# module comment\ndef f(x = lambda y: y + 1):\n    # Documentation follows.\n    \"\"\"doc\"\"\"\n    # implementation comment\n    return x(1)\n",
        "# module comment\ndef f(x = lambda y: y + 1):\n    # Documentation follows.\n    \"\"\"doc\"\"\"\n    ...\n");
}

#[test]
fn docstring_quotes_prefixes_and_escaped_characters_keep_their_original_bytes() {
    for docstring in [
        "'single'",
        "\"double\"",
        "u'Unicode 中文'",
        "r'raw \\n { }'",
        "'''triple single'''",
        "\"\"\"triple double\"\"\"",
        "\"escaped \\\" quote\"",
    ] {
        let source = format!("def f():\n    {docstring}\n    return 1\n");
        let expected = format!("def f():\n    {docstring}\n    ...\n");
        golden("python", false, &source, &expected);
    }
}

#[test]
fn typescript_decorators_abstract_members_and_ambient_namespaces_are_retained() {
    golden("typescript", false,
        "declare namespace API { function load(id: string): Promise<number>; }\n@service()\nexport abstract class Base {\n  abstract load(id: string): Promise<number>;\n  @memoize()\n  concrete(id: string = 'x'): number { return expensive(id); }\n}\n",
        "declare namespace API { function load(id: string): Promise<number>; }\n@service()\nexport abstract class Base {\n  abstract load(id: string): Promise<number>;\n  @memoize()\n  concrete(id: string = 'x'): number { /* ... */ }\n}\n");
}

#[test]
fn unknown_languages_preserve_source_even_when_it_looks_prunable() {
    for language in ["", "Python", "PYTHON", "json", "ruby", "markdown"] {
        let source = "def f():\n    return '世界'\nfunction f() { return 42; }\n";
        assert_eq!(skeletonize(source, language, true), source, "{language}");
    }
}

#[test]
fn malformed_python_and_typescript_are_preserved_in_their_entirety() {
    for (language, jsx, sources) in [
        (
            "python",
            false,
            vec![
                "def broken(\n    return 1\n",
                "def good():\n    return 1\ndef broken():\n    return (\n",
                "value = '''unterminated\n",
            ],
        ),
        (
            "typescript",
            false,
            vec![
                "function broken( {",
                "function good() { return 1; }\nconst broken = ;",
                "const x = `unterminated;",
            ],
        ),
        (
            "javascript",
            true,
            vec![
                "const View = () => <div>;",
                "function good() { return 1; }\nconst broken = <div>",
            ],
        ),
    ] {
        for source in sources {
            assert!(
                has_parse_error(source, language, jsx),
                "Fixture must have an AST error: {source}"
            );
            assert_eq!(
                skeletonize(source, language, jsx),
                source,
                "{language}: {source}"
            );
        }
    }
}

#[test]
fn lexical_fallback_preserves_unbalanced_braces_and_unterminated_literals() {
    for source in [
        "fn good() {}\nfn broken() {",
        "fn broken() {}}",
        "fn f() { let x = \"unterminated; }",
        "fn f() { /* unterminated }",
        "fn f() { /* outer /* inner */ }",
        "fn f() { let x = r###\"no terminator\"##; }",
        "fn f() { let x = br##\"no terminator\"#; }",
        "int f() { auto s = R\"tag(unterminated)wrong\"; }",
        "func f() { x := `unterminated }",
    ] {
        assert_eq!(skeletonize(source, "rust", false), source, "{source}");
    }
}

#[test]
fn lexical_fallback_preserves_initializers_macros_and_control_flow() {
    for source in [
        "const DATA = make() { 1 };",
        "macro_rules! items { () => { 1 } }",
        "struct Data { int value; };",
        "enum State { Ready, Busy }",
        "trait Api { fn read(&self); }",
        "if (ready) { call(); } else { stop(); }",
        "for (int i = 0; i < 2; i++) { call(); }",
        "while (ready) { call(); }",
        "switch (value) { case 1: break; }",
        "#define CALL(x) { x; }",
    ] {
        assert_eq!(skeletonize(source, "cpp", false), source, "{source}");
    }
}

#[test]
fn lexical_literals_do_not_affect_braces_or_utf8_offsets() {
    for literal in [
        "\"escaped \\\" } {\"",
        "'\\''",
        "'\\u{1f43e}'",
        "'猫'",
        "b'}'",
        "r\"} {\"",
        "r#####\"} \"### {\"#####",
        "br##\"} {\"##",
        "R\"(} {)\"",
        "R\"custom(} \" {)custom\"",
        "`} { // /*`",
    ] {
        let source = format!("// 中文 🐾\nfn first() {{ let value = {literal}; /* {{ /* }} */ {{ */ }}\nfn next() {{ return; }}\n");
        golden(
            "rust",
            false,
            &source,
            "// 中文 🐾\nfn first() { /* ... */ }\nfn next() { /* ... */ }\n",
        );
    }
}

#[test]
fn crlf_outside_pruned_ranges_and_signature_bytes_are_preserved() {
    for (language, source, expected) in [
        (
            "python",
            "# doc\r\ndef f(\r\n    x: int,\r\n):\r\n    return x\r\n\r\nVALUE = 1\r\n",
            "# doc\r\ndef f(\r\n    x: int,\r\n):\r\n    ...\r\n\r\nVALUE = 1\r\n",
        ),
        (
            "typescript",
            "// doc\r\nfunction f(\r\n x: number\r\n) { return x; }\r\nconst next = 1;\r\n",
            "// doc\r\nfunction f(\r\n x: number\r\n) { /* ... */ }\r\nconst next = 1;\r\n",
        ),
        (
            "rust",
            "// doc\r\nfn f(\r\n x: i32\r\n) { let _ = x; }\r\n",
            "// doc\r\nfn f(\r\n x: i32\r\n) { /* ... */ }\r\n",
        ),
    ] {
        golden(language, false, source, expected);
    }
}

#[test]
fn many_independent_functions_keep_every_signature_and_remove_every_body() {
    for language in ["python", "typescript", "rust"] {
        let mut source = String::new();
        let mut expected = String::new();
        for index in 0..256 {
            let (input, output) = match language {
                "python" => (
                    format!("def f_{index}(x: int = {index}):\n    return x + {index}\n\n"),
                    format!("def f_{index}(x: int = {index}):\n    ...\n\n"),
                ),
                "typescript" => (
                    format!("export const f_{index} = (x: number = {index}) => x + {index};\n"),
                    format!("export const f_{index} = (x: number = {index}) => {{ /* ... */ }};\n"),
                ),
                _ => (
                    format!("pub fn f_{index}(x: i32) -> i32 {{ x + {index} }}\n"),
                    format!("pub fn f_{index}(x: i32) -> i32 {{ /* ... */ }}\n"),
                ),
            };
            source.push_str(&input);
            expected.push_str(&output);
        }
        golden(language, false, &source, &expected);
    }
}

#[test]
fn adversarial_utf8_inputs_never_panic_and_unknown_language_round_trips() {
    // Deterministic corpus: reproduce any failing case without a seed or dependency.
    let alphabet = [
        'a', ' ', '\n', '\r', '\t', '{', '}', '[', ']', '(', ')', '\'', '"', '`', '/', '*', '\\',
        '#', ':', ';', '中', '🐾', '\0',
    ];
    let mut state = 0x5eed_u64;
    for case in 0..512 {
        let mut source = String::new();
        for _ in 0..case % 97 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push(alphabet[(state >> 32) as usize % alphabet.len()]);
        }
        for language in [
            "python",
            "typescript",
            "javascript",
            "rust",
            "go",
            "java",
            "c",
            "cpp",
        ] {
            let output = skeletonize(&source, language, case % 2 == 0);
            if matches!(language, "python" | "typescript" | "javascript") {
                if has_parse_error(&source, language, case % 2 == 0) {
                    assert_eq!(
                        output, source,
                        "Invalid input changed in case {case}: {language}"
                    );
                } else {
                    assert_parses(&output, language, case % 2 == 0);
                }
            }
            assert_eq!(
                skeletonize(&output, language, case % 2 == 0),
                output,
                "Non-idempotent case {case}: {language}, {source:?}"
            );
        }
        assert_eq!(skeletonize(&source, "unknown", false), source);
    }
}
