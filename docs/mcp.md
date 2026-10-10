# skel MCP 使用说明

`skel` 的 MCP 模式通过标准输入输出提供两个只读工具。客户端启动服务进程后，就能直接获取项目的代码骨架，无需先生成 `context.md`。

## 构建和启动

在项目目录运行：

```powershell
cargo build --release --locked
.\target\release\skel.exe C:\path\to\project --mcp
```

启动目录是服务允许读取的范围，必须是目录。工具参数中的相对路径以这个目录为基准；目录外的绝对路径、`..` 越界路径，以及指向目录外文件的符号链接都会被拒绝。

服务启动后等待客户端的 MCP 请求，不会显示 CLI 的压缩报告或预览。标准输出只用于协议，错误信息写到标准错误。客户端关闭连接后服务退出。

## 客户端配置

在支持 `mcpServers` 配置的客户端中加入下面的内容，将两个路径替换成实际的**绝对路径**。JSON 中的 Windows 路径可以使用 `/`，也可以使用转义后的 `\\`。

```json
{
  "mcpServers": {
    "skel": {
      "command": "C:/path/to/skel/target/release/skel.exe",
      "args": ["C:/path/to/project", "--mcp"]
    }
  }
}
```

`command` 是可执行文件路径，`args` 的第一个元素是要分析的项目目录。使用绝对路径可以避免客户端工作目录不同导致的启动问题。路径中含有空格时，也保持为一个 JSON 字符串，不必额外嵌入引号。

其他客户端使用自己的 MCP 配置格式时，填写相同的 executable 和 arguments，传输方式选择 stdio。服务不监听 HTTP 端口。

## 工具

| 工具 | 用途 |
| --- | --- |
| `skel_list_files` | 列出支持的源码文件、语言和 focus 匹配状态，不读取源码内容 |
| `skel_skeletonize` | 生成 Markdown 骨架上下文，附带结构化统计、文件信息和警告 |

两个工具都接受以下参数：

```json
{
  "path": "src",
  "focus": ["auth.py", "services/**"]
}
```

`path` 可以是文件或目录，省略时扫描整个服务根目录。`focus` 省略时为空数组。上例的匹配对象是 `src` 目录下的相对路径，例如 `src/auth.py` 对应 `auth.py`。选择单个文件时，focus 匹配文件名。

获取骨架的 JSON-RPC 请求示例（客户端通常会自动构造）：

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "skel_skeletonize",
    "arguments": {
      "path": "src",
      "focus": ["auth.py"]
    }
  }
}
```

调用前，客户端需要完成 `initialize` / `notifications/initialized` 握手。

## 返回内容和限制

骨架工具在 `content` 中返回 Markdown，在 `structuredContent` 中返回 `stats`、`files`、`warnings` 和 `markdown_estimated_tokens`。文件模式含义如下：

- `FULL`：focus 匹配，保留全部源码。
- `SKELETON`：裁剪后的内容与原文不同。
- `UNCHANGED`：没有裁剪，可能只有声明，也可能解析失败；不能据此认定解析成功。

Token 数量按 UTF-8 字节数除以 3.5 估算，不是模型 tokenizer 的精确结果。`stats` 只统计源码；`markdown_estimated_tokens` 统计完整 Markdown，包含标题、文件索引和代码围栏，不包含 MCP/JSON 包装。

每次调用最多处理 1,000 个源码文件、每文件 2 MiB、总输入 16 MiB，序列化后的工具响应最多 2 MiB。扫描时发现第 1,001 个源码文件便会立即停止。超过限制会返回 `isError: true`，不会截断内容；可缩小 `path` 范围后重试。

两个工具都会在 `warnings` 中报告目录遍历或文件元数据读取失败，保留已发现的有效文件；扫描失败且没有发现文件时返回工具错误。骨架工具还会报告源码读取失败或非 UTF-8 文件，全部源码读取失败则返回工具错误。工具参数的类型或字段错误也会作为工具错误返回；未知工具名称属于 JSON-RPC 错误。一次调用失败后，服务仍能处理后续请求。Python、TypeScript/JavaScript 的 AST 使用迭代遍历，深层嵌套不会耗尽工作线程的调用栈。

当前沿用 CLI 的文件过滤：排除隐藏目录以及 `node_modules`、`target` 等固定目录，不读取 `.gitignore`。Python、TypeScript/JavaScript 使用 AST；Go、Rust、Java、C/C++ 使用现有的词法裁剪，复杂语法仍有已知限制。工具不会修改或生成项目文件。

## 验证

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

MCP 集成测试会启动真实 `skel --mcp` 子进程，验证握手、工具发现、骨架和统计调用、focus、错误恢复、读取范围、输出围栏和连接关闭。
