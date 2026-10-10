# 测试说明

测试仅使用现有 Rust 依赖，不需要 Python、Node.js、网络服务或额外测试依赖。
集成测试由 Cargo 自动发现，通过 `CARGO_BIN_EXE_skel` 启动本次构建的程序。

## 运行

```powershell
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

单独运行某组测试：

```powershell
cargo test --locked --test languages
cargo test --locked --test formatting
cargo test --locked --test walker
cargo test --locked --test cli
cargo test --locked --test mcp
```

## 覆盖范围

| 测试文件 | 检查内容 |
| :--- | :--- |
| `languages.rs` | Python 签名、装饰器、异步函数、类、文档字符串、嵌套函数；TS/JS 泛型、重载、访问器、箭头函数、生成器声明、装饰器、TSX/JSX；各类词法回退、字符串和注释中的括号、畸形输入、Unicode、CRLF、文件末尾、重复处理 |
| `formatting.rs` | token 估算的字节语义与舍入边界、统计汇总、零输入、输出膨胀、费用计算、千分位、完整 Markdown 和终端报告、嵌入代码围栏 |
| `walker.rs` | 全部扩展名、大小写、元数据和排序、每一条默认忽略规则、显式选择被忽略目录、单文件、focus 联集和路径语义、非法 glob、文件数边界、Unicode 路径、`.gitignore` 行为 |
| `cli.rs` | 帮助和版本、默认目录、混合语言输出、重复 focus、单文件、统计模式、预览的 40 行边界、空文件、无支持文件、非法参数、缺失路径、非 UTF-8、写入失败、已有输出保护、来源文件完整性 |
| `mcp.rs` | stdio 握手、工具 schema 和只读声明、全部参数类型、默认值、路径边界、兄弟目录前缀、文件模式、完整元数据与统计、错误后恢复、并发请求 ID、每次读取最新源文件、只读快照、深层 AST、资源限制 |
| `regressions.rs` | 保留原有忽略祖先、缺失扫描根、focus 错误、CLI 焦点文件回归 |
| `known_bugs.rs` | 历史已知缺陷回归用例；缺陷已全部修复并纳入默认回归测试套件 |

语言测试比较精确预期结果，并检查输出语法和重复处理的一致性；词法回退不声称具有完整的语言语法校验。
额外覆盖 256 个连续函数，以及固定种子的 512 组 UTF-8 混合输入。
畸形 AST 输入必须原样返回，有效 AST 输入的输出必须仍可解析。

MCP 边界测试检查：单文件 2 MiB 以下、恰好 2 MiB、超出 1 字节；
总输入恰好 16 MiB 和超出 1 字节；恰好 1,000 文件和第 1,001 个文件；
以及 JSON 换行转义导致响应超过 2 MiB 的情况。focus 不能绕过源文件大小限制。

## 已修复的主程序缺陷回归

下列 8 项历史缺陷均已在主程序中修复，用例保留正确的预期结果，并已移除 `ignore` 纳入默认回归测试套件。

| 用例 | 修复内容 |
| :--- | :--- |
| `python_f_strings_are_implementation_expressions_not_docstrings` | 函数首行 f-string 不再误判为文档字符串，表达式实现被正确裁剪 |
| `python_bytes_literals_are_not_docstrings` | 函数首行 bytes 字面量不再误判为文档字符串，已被正确裁剪 |
| `python_implicitly_concatenated_docstrings_are_preserved` | 隐式拼接形成的真实文档字符串被正确保留 |
| `python_parenthesized_docstrings_are_preserved` | 括号包裹的真实文档字符串被正确保留 |
| `python_missing_function_bodies_are_retained_as_invalid_input` | 校验空函数体/空类体，缺失函数体等畸形 Python 输入被原样保留 |
| `javascript_generator_function_expressions_have_their_bodies_pruned` | 支持 `function*` 表达式 AST 节点裁剪 |
| `markdown_file_paths_with_backticks_use_longer_inline_delimiters` | 文件名里的反引号使用更长的行内反引号定界符包裹 |
| `markdown_file_paths_with_pipes_are_escaped_in_summary_tables` | 表格列中文件名里的管道符 `\|` 被正确转义 |

运行全部回归测试：

```powershell
cargo test --locked --test known_bugs
```

## 测试进程与平台

`support/` 提供独立临时目录、路径受限的清理、目录及文件字节快照、子进程退出回收，
以及同时读取 stdout/stderr 的执行器。CLI 子进程和每次 MCP 响应均设 20 秒期限，
MCP 收到 EOF 后须在 5 秒内正常退出。通知不会延长响应期限，乱序响应按 ID 缓存。
测试失败时也会关闭管道并终止、回收 MCP 子进程。

测试不修改当前进程的工作目录或环境变量，可以并行运行。
符号链接相关用例通过 `cfg(unix)` 编译在 Unix 平台运行；Windows 上不计入运行数量。
权限不足和特定平台文件系统失败没有被当作可移植的普通测试条件。
Tree-sitter 语法检查不是 Python 语义校验或 TypeScript 类型检查；
固定语料也不能替代持续模糊测试，因此不据此声称达到 100% 覆盖率。
