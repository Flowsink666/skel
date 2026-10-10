# 🦴 skel (Skeletonizer)

> **AST-aware Code Skeletonizer for LLM Prompts & Agent Contexts**  
> *Reduce codebase context by pruning implementation bodies while retaining declarations, signatures, and docstrings.*

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust: 1.90+](https://img.shields.io/badge/rust-1.90+-orange.svg)](https://www.rust-lang.org)

---

## ⚡ Why `skel`? (The Problem with Code Packers)

Tools like **Repomix** pack your entire codebase into a single file for AI prompts. But on real-world projects, this hits major walls:
1. **Token Usage**: Packing full implementations increases prompt size, especially in repositories with large function bodies.
2. **Context Dilution**: Loops, error handling, and helper implementations can obscure the interfaces relevant to a task.
3. **File Selection**: Excluding a whole file also removes its types and method signatures from the context.

### The `skel` Solution: The Context Pyramid

`skel` uses AST extraction for Python and TypeScript/JavaScript in the Rust CLI:
* **Keeps**: Interfaces, types, structs, exported signatures, parameter annotations, and docstrings.
* **Prunes**: Replaces function and method bodies with placeholders (`...` or `{ /* ... */ }`). These are context summaries, not runnable or type-checked implementations.
* **Progressive Focus (`--focus`)**: Keep 100% of the full code for files you are actively working on, while skeletonizing all dependencies.

```
┌──────────────────────────────────────────────┐
│  Tier 1: Focus Files (100% Full Code)        │  <-- e.g. src/auth/jwt.ts
├──────────────────────────────────────────────┤
│  Tier 2: Background Files (AST Skeletons)    │  <-- Reduced implementation detail
├──────────────────────────────────────────────┤
│  Tier 3: Repository Structure & File Index   │
└──────────────────────────────────────────────┘
```

---

## Supported Behavior

| Feature | Rust CLI |
| :--- | :--- |
| Python | Tree-sitter AST; keeps docstrings and original signature text |
| TypeScript / JavaScript | Tree-sitter AST; prunes block and expression arrow bodies, including TSX/JSX |
| Go / Rust / Java / C / C++ | Conservative lexical handling |
| Focus files | Full source for matching `--focus` globs; invalid globs return an error |
| MCP server | Read-only stdio tools via `skel <root> --mcp` |
| Compression | Depends on implementation size, language, and focus selection |

The implementation and tests are written in Rust. No Python interpreter is
required to build, run, or test `skel`. The `src/python.rs` module and
`tree-sitter-python` dependency parse Python source code; files under
`examples/python_service` are sample inputs.

The lexical fallback masks strings and comments before matching complete brace
pairs. It retains unbalanced input unchanged and may leave unfamiliar constructs
unpruned. It is not a complete parser for these languages and cannot guarantee
syntax for every language feature. Invalid Python/TypeScript AST input in the
Rust CLI is also retained unchanged.

Ignore rules apply **within the selected root**. Hidden or ignored ancestor
folders do not prevent scanning an explicitly selected project; ignored child
folders such as `node_modules`, `.git`, and `target` are skipped.

Token counts use a character/byte heuristic rather than a model tokenizer. Cost
savings use a fixed illustrative rate of $3 per million input tokens, not a live
model price. Markdown wrapper overhead is not included in compression statistics.

---

## 🚀 Quick Start

### 1. Build and Run
Powered by native Tree-sitter parsers:

```bash
# Build release binary
cargo build --release --locked

# Run on current directory and generate context.md
./target/release/skel . -o context.md

# Focus on specific files (keep full implementation for auth, skeletonize everything else)
./target/release/skel . --focus "*/auth*" -o context.md

# Inspect compression stats without outputting files
./target/release/skel . --stats
```

On Windows, run the release executable from PowerShell:

```powershell
.\target\release\skel.exe . -o context.md
```

### 2. Sample Output in Terminal (Illustrative)

```text
============================================================
 [skel v0.2.0] Codebase Context Compression Report
------------------------------------------------------------
 Files processed:    28
 Original tokens:    48,210
 Skeleton tokens:    4,680
 Compression ratio:  90.3% reduction
 Est. cost saved:    $0.1306 (Claude 3.5 Sonnet tier)
============================================================
[OK] Context written to: context.md (18,420 bytes)
```

---

## MCP Server

Build the same executable and launch it as a local stdio MCP server:

```bash
cargo build --release --locked
./target/release/skel /absolute/path/to/project --mcp
```

The server uses the [official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk).
Its stdout contains only MCP protocol messages. The client launches the process
and owns its lifetime; errors go to stderr. The startup path must be a directory.
`--mcp` cannot be combined with CLI `--output`, `--stats`, or `--focus`; focus is
provided in tool arguments instead.

| Tool | Result |
| :--- | :--- |
| `skel_list_files` | Supported file paths, languages, and focus matches |
| `skel_skeletonize` | Markdown context plus structured file metadata, warnings, and estimated token counts |

Both tools accept `path` (default `"."`) and `focus` (default `[]`). Paths are scoped
to the startup directory, including canonicalized file symlinks. Focus patterns
are relative to the selected directory, or the basename for a single-file input.
The tools never write files. Existing CLI ignore rules apply; `.gitignore` is not
read. Python/TS parse failures retain full source with mode `UNCHANGED`.

Requests are limited to 1,000 supported files, 2 MiB per source file, 16 MiB total
source input, and 2 MiB serialized tool response. Scanning stops as soon as the
source file count exceeds the limit. Exceeding a limit returns a tool error
without truncating content. Both tools report directory traversal failures in
`warnings`; skeletonization also reports unreadable/non-UTF-8 files. A failed
scan with no discovered files, or an input with no readable source files, is an
error. Python/TS/JS AST traversal is iterative, so deeply nested syntax does not
consume the worker's call stack.

See [MCP setup and examples (中文)](docs/mcp.md) and the
[client configuration example](docs/mcp-servers.json).

---

## 📂 Example: Before & After

### Original Code:
```python
class AuthService:
    def __init__(self, token_ttl: int = 3600):
        self.token_ttl = token_ttl
        self._cache = {}

    def hash_password(self, password: str, salt: str) -> str:
        """Hash plain password using PBKDF2 HMAC SHA256 with 100k iterations."""
        pwd_bytes = password.encode("utf-8")
        salt_bytes = salt.encode("utf-8")
        derived = hashlib.pbkdf2_hmac("sha256", pwd_bytes, salt_bytes, 100000)
        return derived.hex()

    def verify_password(self, plain: str, salt: str, expected_hash: str) -> bool:
        """Constant-time comparison to prevent timing attacks."""
        calculated = self.hash_password(plain, salt)
        return hmac.compare_digest(calculated, expected_hash)
```

### Skeleton Output:
```python
class AuthService:

    def __init__(self, token_ttl: int = 3600):
        ...

    def hash_password(self, password: str, salt: str) -> str:
        """Hash plain password using PBKDF2 HMAC SHA256 with 100k iterations."""
        ...

    def verify_password(self, plain: str, salt: str, expected_hash: str) -> bool:
        """Constant-time comparison to prevent timing attacks."""
        ...
```

The context retains the class interface and method documentation while omitting the hashing implementation.

---

## Development Checks

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

The regression suite covers literal/comment braces, nested blocks, multiline
signatures, Python inline and Tab-indented docstrings, expression arrow functions,
ignored ancestors, focus matching, and invalid Rust CLI focus patterns.

The crate declares Rust 1.90 as its minimum version to match the locked dependency
requirements.

Library callers must handle the `Result` returned by `discover_files` and
`build_focus_globset`; invalid focus patterns are propagated as errors.

---

## 🗺️ Roadmap

- [x] Python AST Skeletonizer (retaining docstrings & types)
- [x] TypeScript / JavaScript brace-aware skeletonizer
- [x] Mixed context mode (`--focus` flag)
- [x] Tree-sitter core engine rewrite in Rust (Python & TS/JS support)
- [ ] WebAssembly (WASM) build
- [ ] Reverse Token Budgeting (`--max-tokens 32k` adaptive reduction)
- [x] Native **Model Context Protocol (MCP)** server integration (`skel --mcp`, stdio)
- [ ] Interactive TUI file picker (`skel --interactive`)

---

## 📄 License
MIT
