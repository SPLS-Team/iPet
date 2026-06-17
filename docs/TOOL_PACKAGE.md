# iPet 工具包导入协议（schemaVersion 1）

本文档定义把一个外部工具引入 iPet 的最小可用格式。当前 v1 仅支持 HTTP 类型
工具，未来版本会扩展 `rust/` 内置和权限模型。

## 包结构

```
my-tool/
  tool.json        # 必须，元数据与参数 schema
  README.md        # 可选，展示在 UI 详情区
```

可以是一个目录，也可以直接是一个 `tool.json` 文件。zip 暂不支持，先解压。

## `tool.json` 字段

| 字段              | 类型     | 必填 | 说明 |
|------------------|----------|------|------|
| `schemaVersion`  | int      | ✅   | 当前固定为 `1`，不匹配会被拒绝。 |
| `name`           | string   | ✅   | 工具内部标识，必须满足 `[_A-Za-z][_A-Za-z0-9]*`，与已有工具同名时覆盖（内置工具除外）。 |
| `displayName`    | string   | ✅   | 用户可见名。 |
| `description`    | string   | ✅   | 一行描述，会作为 `function.description` 传给模型。 |
| `version`        | string   | 可选 | 语义化版本，仅用于审计/UI 展示，不影响运行。 |
| `kind`           | string   | ✅   | 当前必须是 `"http"`。 |
| `parameters`     | object   | ✅   | 合法的 JSON Schema，且 `type` 必须为 `"object"`。 |
| `http.method`    | string   | ✅   | `GET` / `POST` / `PUT` / `PATCH` 之一。 |
| `http.url`       | string   | ✅   | 必须 `http://` 或 `https://` 开头，且不能解析到 loopback / 私有 / 链路本地等受限地址（运行时还会再校验一次 DNS）。 |
| `http.headers`   | array    | 可选 | `[{"key":"...","value":"..."}]`。 |
| `permissions`    | string[] | 可选 | 申明用途，例如 `["network"]`，目前仅做记录。 |
| `enabled`        | bool     | 可选 | 默认 `true`。 |

## 完整示例

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

## 后端导入接口

```ts
await invoke("import_tool_from_path", { path: "C:/.../my-tool" });
// or
await invoke("import_tool_from_path", { path: "C:/.../my-tool/tool.json" });
```

返回值是 `ToolConfig`（即 `list_tools` 中的同款结构）。失败时返回错误字符串，
常见原因：

- `tool.json` 缺失或不是合法 JSON
- `schemaVersion` 不是 `1`
- 必填字段缺失
- `parameters.type` 不是 `"object"`
- `http.url` 不合法或指向受限地址
- 与内置工具同名

## 安全说明

导入工具会立刻把它写入 SQLite，模型在下一轮对话中即可调用。请只导入信任来源
的包；HTTP 工具的请求由 Rust 端通过共享 `http_safety::validate_url_runtime`
做 SSRF 防护（loopback / 私有 / 链路本地一律拦截），并加超时和响应体上限。
