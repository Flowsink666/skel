# 🦴 skel (Skeletonizer)

> **AST-aware Code Skeletonizer for LLM Prompts & Agent Contexts**  
> *Compress your codebase tokens by 80–95% while keeping 100% of types, signatures, and docstrings.*

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Python: 3.9+](https://img.shields.io/badge/python-3.9+-brightgreen.svg)](https://python.org)

---

## ⚡ Why `skel`? (The Problem with Code Packers)

Tools like **Repomix** pack your entire codebase into a single file for AI prompts. But on real-world projects, this hits major walls:
1. **Expensive Token Waste**: A 200-file project consumes 100k–300k tokens. Every chat query costs real money ($0.30–$1.00+ on Claude 3.5 Sonnet / GPT-4o).
2. **Context Dilution ("Lost in the Middle")**: 90% of code lines are internal implementation details (loops, error handling, helper functions). LLMs get distracted and hallucinate when overwhelmed by implementation noise.
3. **The "All-or-Nothing" Dilemma**: Repomix either includes the full file or excludes it entirely. If you exclude a file, the AI loses critical type interfaces and method signatures.

### The `skel` Solution: The Context Pyramid

`skel` performs **AST-level surgical extraction**:
* **Keeps**: Interfaces, types, structs, exported signatures, parameter annotations, and docstrings.
* **Prunes**: Replaces function and method bodies with syntactically valid placeholders (`...` or `{ /* ... */ }`).
* **Progressive Focus (`--focus`)**: Keep 100% of the full code for files you are actively working on, while skeletonizing all dependencies.

```
┌──────────────────────────────────────────────┐
│  Tier 1: Focus Files (100% Full Code)        │  <-- e.g. src/auth/jwt.ts
├──────────────────────────────────────────────┤
│  Tier 2: Background Files (AST Skeletons)    │  <-- 80-95% Token Reduction!
├──────────────────────────────────────────────┤
│  Tier 3: Repository Structure & File Index   │
└──────────────────────────────────────────────┘
```

---

## 📊 Comparison with Repomix

| Feature | Repomix | `skel` |
| :--- | :--- | :--- |
| **Philosophy** | Code Packer (pack everything) | **Intelligent Context Pruner** |
| **Compression** | Removes comments & empty lines | **Empties implementation bodies via AST** |
| **Docstrings** | Deleted when stripping comments ❌ | **Preserved (critical for AI reasoning) ✅** |
| **Syntax Validity** | Plain text minification | **100% syntactically valid code** |
| **Progressive Focus** | ❌ (All files treated equally) | **✅ (`--focus "src/api/*"`)** |
| **Token Reduction** | ~10% – 20% | **~80% – 95%** |
| **MCP Integration** | External CLI only | **Native MCP tool coming soon** |

---

## 🚀 Quick Start

### 1. Zero-dependency CLI
`skel` is built with zero external dependencies.

```bash
# Run on current directory and generate context.md
python skel.py . -o context.md

# Focus on specific files (keep full implementation for auth, skeletonize everything else)
python skel.py . --focus "*/auth*" -o context.md

# Inspect compression stats without outputting files
python skel.py . --stats
```

### 2. Sample Output in Terminal

```text
============================================================
 [skel v0.1.0] Codebase Context Compression Report
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

## 📂 Example: Before & After

### Original Code (52 lines):
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

### Skeleton Output (14 lines, 100% syntactically valid):
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

The LLM knows **exactly what the class does and how to call it**, without paying tokens for lines of hashing algorithms!

---

## 🗺️ Roadmap

- [x] Python AST Skeletonizer (retaining docstrings & types)
- [x] TypeScript / JavaScript brace-aware skeletonizer
- [x] Mixed context mode (`--focus` flag)
- [x] Token savings calculator ($USD & %)
- [ ] Tree-sitter core engine rewrite (Rust / WASM)
- [ ] Reverse Token Budgeting (`--max-tokens 32k` adaptive reduction)
- [ ] Native **Model Context Protocol (MCP)** server integration (`skel --mcp`)
- [ ] Interactive TUI file picker (`skel --interactive`)

---

## 📄 License
MIT
