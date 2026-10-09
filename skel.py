#!/usr/bin/env python3
"""
skel - AST-aware & Signature-level Code Skeletonizer for LLM Context
Compress repository context by 80-95% while keeping 100% of types, signatures, and docstrings.
"""

import ast
import argparse
import fnmatch
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple

# Ensure UTF-8 output on Windows console
if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8")
        sys.stderr.reconfigure(encoding="utf-8")
    except Exception:
        pass

VERSION = "0.1.0"

# Default directories and files to ignore
DEFAULT_IGNORES = {
    ".git", ".svn", ".hg", "node_modules", "__pycache__", ".venv", "venv", "env",
    "dist", "build", "target", ".next", ".nuxt", ".cache", ".idea", ".vscode",
    "coverage", ".pytest_cache", ".mypy_cache", ".tox", "vendor"
}

# Supported file extensions and their language types
EXTENSIONS = {
    ".py": "python",
    ".ts": "typescript",
    ".tsx": "typescript",
    ".js": "javascript",
    ".jsx": "javascript",
    ".go": "go",
    ".rs": "rust",
    ".java": "java",
    ".c": "c",
    ".cpp": "cpp",
    ".h": "c",
    ".hpp": "cpp",
}


class PythonSkeletonizer(ast.NodeTransformer):
    """
    Python AST Transformer:
    - Retains class declarations, methods, decorators, and type annotations
    - Retains docstrings (critical for LLM comprehension)
    - Replaces implementation bodies with Ellipsis (...)
    """

    def _prune_body(self, node):
        docstring_node = None
        if (node.body and isinstance(node.body[0], ast.Expr) and
                isinstance(node.body[0].value, ast.Constant) and
                isinstance(node.body[0].value.value, str)):
            docstring_node = node.body[0]

        ellipsis_node = ast.Expr(value=ast.Constant(value=Ellipsis))

        if docstring_node:
            node.body = [docstring_node, ellipsis_node]
        else:
            node.body = [ellipsis_node]
        return node

    def visit_FunctionDef(self, node):
        self.generic_visit(node)
        return self._prune_body(node)

    def visit_AsyncFunctionDef(self, node):
        self.generic_visit(node)
        return self._prune_body(node)


def skeletonize_python(code: str) -> str:
    """Parse Python code with native AST and generate skeleton."""
    try:
        tree = ast.parse(code)
        pruned_tree = PythonSkeletonizer().visit(tree)
        ast.fix_missing_locations(pruned_tree)
        return ast.unparse(pruned_tree)
    except SyntaxError:
        # Fallback to original code if syntax error
        return code


def skeletonize_c_like(code: str, lang: str = "typescript") -> str:
    """
    Grammar & bracket-aware pruner for C-family/curly-brace languages (TS/JS/Go/Rust/Java/C++).
    - Preserves imports, exports, types, interfaces, structs, enums, docstrings
    - Replaces function bodies {...} with {/* ... */} or /* ... */
    """
    lines = code.splitlines()
    result = []
    
    in_block_comment = False
    in_function_body = False
    brace_depth = 0
    func_start_depth = 0
    pending_comment = []

    # Patterns indicating a signature/declaration
    fn_keyword_pattern = re.compile(
        r'^\s*(export\s+)?(async\s+)?(public\s+|private\s+|protected\s+|static\s+|override\s+|fn\s+|func\s+|def\s+)*'
        r'(function\s+|fn\s+|func\s+|constructor\s*\(|[a-zA-Z0-9_$]+\s*\([^)]*\)\s*(:\s*[^={]+)?\s*\{?)'
    )
    
    # Types / Interfaces / Structs should be kept intact
    type_decl_pattern = re.compile(
        r'^\s*(export\s+)?(type|interface|enum|struct|trait|impl|class)\b'
    )

    for line in lines:
        stripped = line.strip()

        # Preserve JSDoc / block comments
        if "/*" in line and "*/" not in line:
            in_block_comment = True
            result.append(line)
            continue
        if in_block_comment:
            result.append(line)
            if "*/" in line:
                in_block_comment = False
            continue

        # Preserve single-line comments/docstrings
        if stripped.startswith("//") or stripped.startswith("#"):
            result.append(line)
            continue

        # Count braces (ignoring strings crudely)
        open_braces = line.count("{") - line.count("\\{")
        close_braces = line.count("}") - line.count("\\}")

        if not in_function_body:
            # Check if this line starts a function implementation
            is_type = bool(type_decl_pattern.match(stripped))
            is_fn = bool(fn_keyword_pattern.match(stripped)) and not is_type

            if is_fn and "{" in line and not stripped.endswith(";"):
                # If function opens on this line
                brace_pos = line.rfind("{")
                sig_part = line[:brace_pos].rstrip()
                
                # Check if it also closes on same line (one-liner)
                if "}" in line:
                    result.append(f"{sig_part} {{ /* ... */ }}")
                else:
                    in_function_body = True
                    func_start_depth = brace_depth
                    result.append(f"{sig_part} {{ /* ... */ }}")
                    brace_depth += (open_braces - close_braces)
                continue
            
            result.append(line)
            brace_depth += (open_braces - close_braces)
        else:
            # Inside function body: skip lines until matching closing brace
            brace_depth += (open_braces - close_braces)
            if brace_depth <= func_start_depth:
                in_function_body = False

    return "\n".join(result)


def skeletonize(code: str, lang: str) -> str:
    """Route code to language-specific skeletonizer."""
    if lang == "python":
        return skeletonize_python(code)
    elif lang in ("typescript", "javascript", "go", "rust", "java", "c", "cpp"):
        return skeletonize_c_like(code, lang)
    return code


def estimate_tokens(text: str) -> int:
    """Fast token estimation (~3.5 chars per token for source code)."""
    return max(1, int(len(text) / 3.5))


def should_ignore(path: Path) -> bool:
    for part in path.parts:
        if part in DEFAULT_IGNORES or (part.startswith(".") and part != "."):
            return True
    return False


def is_focus_file(rel_path_str: str, focus_patterns: List[str]) -> bool:
    if not focus_patterns:
        return False
    normalized = rel_path_str.replace("\\", "/")
    return any(fnmatch.fnmatch(normalized, pat) for pat in focus_patterns)


def process_repo(
    root: Path,
    focus_patterns: List[str],
    include_exts: Optional[List[str]] = None
) -> Tuple[List[Dict], Dict]:
    """Process files in the repository and collect metrics."""
    files_to_process = []
    
    if root.is_file():
        files_to_process.append(root)
        base_dir = root.parent
    else:
        base_dir = root
        for p in root.rglob("*"):
            if p.is_file() and not should_ignore(p):
                ext = p.suffix.lower()
                if ext in EXTENSIONS:
                    if include_exts and ext not in include_exts:
                        continue
                    files_to_process.append(p)

    results = []
    total_orig_tokens = 0
    total_skel_tokens = 0

    for file_path in sorted(files_to_process):
        rel_path = file_path.relative_to(base_dir) if base_dir != file_path else Path(file_path.name)
        rel_path_str = str(rel_path).replace("\\", "/")
        ext = file_path.suffix.lower()
        lang = EXTENSIONS.get(ext, "text")

        try:
            content = file_path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue

        orig_tokens = estimate_tokens(content)
        total_orig_tokens += orig_tokens

        is_focus = is_focus_file(rel_path_str, focus_patterns)

        if is_focus:
            mode = "FULL"
            final_content = content
            skel_tokens = orig_tokens
        else:
            mode = "SKELETON"
            final_content = skeletonize(content, lang)
            skel_tokens = estimate_tokens(final_content)

        total_skel_tokens += skel_tokens
        results.append({
            "path": rel_path_str,
            "lang": lang,
            "mode": mode,
            "orig_tokens": orig_tokens,
            "skel_tokens": skel_tokens,
            "content": final_content,
        })

    saved_tokens = max(0, total_orig_tokens - total_skel_tokens)
    ratio = (saved_tokens / total_orig_tokens * 100) if total_orig_tokens > 0 else 0
    # Estimate savings based on $3.00 / 1M input tokens (Claude 3.5 Sonnet / GPT-4o tier)
    est_savings_usd = (saved_tokens / 1_000_000) * 3.0

    stats = {
        "file_count": len(results),
        "orig_tokens": total_orig_tokens,
        "skel_tokens": total_skel_tokens,
        "saved_tokens": saved_tokens,
        "ratio": ratio,
        "savings_usd": est_savings_usd,
    }

    return results, stats


def format_markdown(results: List[Dict], stats: Dict) -> str:
    """Format results into a clean, LLM-optimized Markdown document."""
    lines = [
        "# Codebase Context",
        f"> Generated by `skel` (Tokens compressed from {stats['orig_tokens']:,} to {stats['skel_tokens']:,}, -{stats['ratio']:.1f}%)\n",
        "## Summary of Included Files",
        "| File | Mode | Language | Tokens |",
        "| :--- | :--- | :--- | :--- |"
    ]

    for r in results:
        lines.append(f"| `{r['path']}` | **{r['mode']}** | {r['lang']} | {r['skel_tokens']:,} |")

    lines.append("\n---\n")

    for r in results:
        lines.append(f"### File: `{r['path']}` ({r['mode']})")
        lines.append(f"```{r['lang']}\n{r['content']}\n```\n")

    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        prog="skel",
        description="AST-aware Code Skeletonizer: Compress codebase context by 80-95% for LLMs."
    )
    parser.add_argument("path", default=".", nargs="?", help="Target directory or file (default: current directory)")
    parser.add_argument("-f", "--focus", action="append", default=[], help="Keep full code for matching paths (e.g. -f 'src/auth/*')")
    parser.add_argument("-o", "--output", help="Write output to file (e.g. -o context.md)")
    parser.add_argument("-s", "--stats", action="store_true", help="Print statistics summary only")
    parser.add_argument("-v", "--version", action="version", version=f"skel {VERSION}")

    args = parser.parse_args()
    root = Path(args.path).resolve()

    if not root.exists():
        print(f"Error: Path '{root}' does not exist.", file=sys.stderr)
        sys.exit(1)

    results, stats = process_repo(root, args.focus)

    if not results:
        print("No supported source files found in target directory.")
        return

    # Print summary box safely
    print("\n" + "=" * 60)
    print(f" [skel v{VERSION}] Codebase Context Compression Report")
    print("-" * 60)
    print(f" Files processed:    {stats['file_count']}")
    print(f" Original tokens:    {stats['orig_tokens']:,}")
    print(f" Skeleton tokens:    {stats['skel_tokens']:,}")
    print(f" Compression ratio:  {stats['ratio']:.1f}% reduction")
    print(f" Est. cost saved:    ${stats['savings_usd']:.4f} (Claude 3.5 Sonnet tier)")
    print("=" * 60 + "\n")

    if args.stats:
        return

    md_content = format_markdown(results, stats)

    if args.output:
        out_path = Path(args.output).resolve()
        out_path.write_text(md_content, encoding="utf-8")
        print(f"[OK] Context written to: {out_path} ({len(md_content):,} bytes)")
    else:
        # If output not specified, print first 40 lines as preview
        preview_lines = md_content.splitlines()[:40]
        print("--- Preview (first 40 lines, use -o context.md to save) ---")
        print("\n".join(preview_lines))
        if len(md_content.splitlines()) > 40:
            print("...\n[Truncated. Use `-o context.md` to output full markdown]")


if __name__ == "__main__":
    main()
