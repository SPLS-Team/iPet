# iPet 工具包导入协议（schemaVersion 1 / 2）

本文档定义把一个外部工具引入 iPet 的最小可用格式。

- **v1**：仅支持 `kind: "http"` 工具。
- **v2**：新增 `kind: "local"` —— 本地可执行文件/脚本工具，通过子进程 stdio 调用。
  v2 向后兼容 v1（http 工具仍可用 v1 或 v2）。

内置工具（`scan_disk` / `get_system_status`）走编译期 seed，不通过本协议导入。

## 包结构

```
my-tool/
  tool.json        # 必须，元数据与参数 schema
  README.md        # 可选，展示在 UI 详情区
  my_tool.py       # local 工具的可执行脚本（kind=local 时通常随包附带）
```

可以是一个目录，也可以直接是一个 `tool.json` 文件。zip 暂不支持，先解压。

## `tool.json` 公共字段

| 字段              | 类型     | 必填 | 说明 |
|------------------|----------|------|------|
| `schemaVersion`  | int      | ✅   | `1` 或 `2`。`kind=local` 需要 `2`。 |
| `name`           | string   | ✅   | 工具内部标识，必须满足 `[_A-Za-z][_A-Za-z0-9]*`，与已有工具同名时覆盖（内置工具除外）。 |
| `displayName`    | string   | ✅   | 用户可见名。 |
| `description`    | string   | ✅   | 一行描述，会作为 `function.description` 传给模型。 |
| `version`        | string   | 可选 | 语义化版本，仅用于审计/UI 展示，不影响运行。 |
| `kind`           | string   | ✅   | `"http"` 或 `"local"`。 |
| `parameters`     | object   | ✅   | 合法的 JSON Schema，且 `type` 必须为 `"object"`。 |
| `permissions`    | string[] | 可选 | 申明用途，例如 `["network"]` / `["process.spawn"]`，目前仅做记录。 |
| `enabled`        | bool     | 可选 | 默认 `true`。 |

## `kind: "http"`

| 字段              | 类型     | 必填 | 说明 |
|------------------|----------|------|------|
| `http.method`    | string   | ✅   | `GET` / `POST` / `PUT` / `PATCH` 之一。 |
| `http.url`       | string   | ✅   | 必须 `http://` 或 `https://` 开头，且不能解析到 loopback / 私有 / 链路本地等受限地址（运行时还会再校验一次 DNS）。 |
| `http.headers`   | array    | 可选 | `[{"key":"...","value":"..."}]`。 |

## `kind: "local"`（v2）

本地工具是本机可执行文件或脚本（`.exe` / `.py` / `.js` / `.bat` …）。iPet 在调用时
spawn 一个子进程，把模型参数作为**一行 JSON** 写到子进程 stdin，然后读取 stdout
作为返回值。子进程独立运行，崩了或卡住都由超时兜底，不会拖垮宿主。

| 字段                | 类型     | 必填 | 说明 |
|--------------------|----------|------|------|
| `local.command`    | string   | ✅   | 可执行文件名或路径。绝对路径会校验存在性；相对路径在导入时**按包目录解析为绝对路径**后入库，使工具可随包整体移动。 |
| `local.args`       | string[] | 可选 | 追加在 command 后的静态参数。模型参数走 stdin，不拼进命令行，避免 shell 注入。 |
| `local.cwd`        | string   | 可选 | 子进程工作目录。相对路径同样按包目录解析为绝对路径。 |
| `local.timeoutSecs`| int      | 可选 | 子进程硬超时（秒），默认 `30`，超时即杀进程并报错。 |

### stdio 协议

1. 子进程启动后，iPet 向其 **stdin** 写入一行：模型参数的 JSON 对象（如
   `{"text":"hello"}\n`），随后关闭 stdin（EOF）。
2. 子进程把结果写到 **stdout**。stdout 的全部内容即工具返回值（通常也是 JSON，
   但不强制；模型会直接看到原文）。
3. 退出码非 0 → 报错，错误信息携带 stderr（截断）。
4. 超过 `timeoutSecs` → 杀进程并报错。
5. stdout 超过 2 MiB → 报错（防止失控脚本耗尽内存）。

stderr 仅在失败时用于诊断，成功时被丢弃。

## 完整示例

### HTTP 工具

```json
{
  "schemaVersion": 1,
  "name": "weather_lookup",
  "displayName": "天气查询",
  "description": "调用公开天气服务返回指定城市的当前温度与天气描述。",
  "version": "1.0.0",
  "kind": "http",
  "parameters": {
    "type": "object",
    "required": ["city"],
    "properties": {
      "city": { "type": "string", "description": "城市名，例如 \"杭州\"" }
    }
  },
  "http": {
    "method": "GET",
    "url": "https://api.example.com/v1/weather",
    "headers": [
      { "key": "Accept", "value": "application/json" }
    ]
  },
  "permissions": ["network"],
  "enabled": true
}
```

### 本地工具

包目录结构：

```
echo_local/
  tool.json
  echo_tool.js
```

`tool.json`：

```json
{
  "schemaVersion": 2,
  "name": "echo_local",
  "displayName": "本地回显",
  "description": "把传入的 text 原样返回，演示本地工具。",
  "version": "1.0.0",
  "kind": "local",
  "parameters": {
    "type": "object",
    "required": ["text"],
    "properties": {
      "text": { "type": "string", "description": "要回显的文本" }
    }
  },
  "local": {
    "command": "node",
    "args": ["echo_tool.js"],
    "timeoutSecs": 15
  },
  "permissions": ["process.spawn"],
  "enabled": true
}
```

`echo_tool.js`（随包分发的脚本，读 stdin 一行、写 stdout）：

```js
let input = "";
process.stdin.on("data", (chunk) => (input += chunk));
process.stdin.on("end", () => {
  const { text = "" } = JSON.parse(input || "{}");
  process.stdout.write(JSON.stringify({ echo: text }));
});
```

## 后端导入接口

```ts
await invoke("import_tool_from_path", { path: "C:/.../my-tool" });
// or
await invoke("import_tool_from_path", { path: "C:/.../my-tool/tool.json" });
```

返回值是 `ToolConfig`（即 `list_tools` 中的同款结构）。失败时返回错误字符串，
常见原因：

- `tool.json` 缺失或不是合法 JSON
- `schemaVersion` 不是 `1` / `2`
- 必填字段缺失
- `parameters.type` 不是 `"object"`
- `http.url` 不合法或指向受限地址
- 与内置工具同名

## 安全说明

导入工具会立刻把它写入 SQLite，模型在下一轮对话中即可调用。请只导入信任来源
的包；HTTP 工具的请求由 Rust 端通过共享 `http_safety::validate_url_runtime`
做 SSRF 防护（loopback / 私有 / 链路本地一律拦截），并加超时和响应体上限。

本地工具（`kind: "local"`）会在你的机器上以当前用户权限执行任意可执行文件，
**等同于手动运行该命令**。务必只导入来源可信的本地工具。iPet 的兜底措施：
模型参数走 stdin 而非命令行（避免 shell 注入）、子进程有硬超时、stdout 有 2 MiB
上限、非零退出码即报错——但这些都不能替代"信任来源"这一前提。
