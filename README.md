# iPet

iPet 是一个 Tauri + Rust + WebView 桌宠应用原型，按 `plan.md` 实现了透明桌面窗口、角色动画、系统状态监控、磁盘占用扫描、OpenAI 兼容聊天接口和本地工具调用。

## 运行

```powershell
npm install
npm run tauri:dev
```

## API Key 配置

API key 不写入 `.env`，也不会在代码中硬编码。启动应用后进入 `设置` 页：

1. 在 `API Key` 输入框填入 OpenAI 兼容接口 key。
2. 按需修改 `Base URL`、模型名、temperature、上下文消息数和人设。
3. 点击 `保存设置`。

设置会保存在应用数据目录的 SQLite 数据库中。设置页只显示“已配置/未配置”，不会回显已保存的 key；留空保存会保留原 key，勾选“清除已保存的 API Key”会删除本地 key。

默认接口：

- Base URL: `https://api.openai.com/v1`
- Model: `gpt-4.1-mini`

## 已实现模块

- 透明、无边框、默认置顶的 Tauri 桌面窗口。
- CSS 动画桌宠，支持待机、思考、说话状态切换，后续可替换为 Live2D 模型。
- 前端聊天面板，接收后端 `chat-stream` 事件做增量显示。
- Rust `sysinfo` 系统监控，作为可启停工具暴露给模型。
- Rust 并行目录扫描，按大小排序返回目录树摘要。
- SQLite 持久化：LLM 设置、工具配置、对话历史、token 使用量、系统采样、磁盘扫描缓存。
- OpenAI 兼容 `/chat/completions` 客户端，支持 function calling 调用启用状态的工具。
- 设置页内 API key、模型、工具、统计和窗口行为配置。

## 工具格式

工具在 `设置 -> 工具` 中管理。内置工具可以启用/停用，自定义工具当前支持 HTTP 调用：

```json
{
  "name": "search_docs",
  "displayName": "搜索文档",
  "description": "当需要查询内部文档时调用。",
  "kind": "http",
  "enabled": true,
  "parameters": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "搜索关键词"
      }
    },
    "required": ["query"]
  },
  "http": {
    "method": "POST",
    "url": "https://example.com/tool",
    "headers": []
  }
}
```

约束：

- `name` 必须是函数名格式，只能包含英文字母、数字和下划线，且不能以数字开头。
- `parameters` 必须是 JSON Schema object。
- HTTP 工具支持 `GET`、`POST`、`PUT`、`PATCH`。
- `GET` 会把模型参数转成 query，其他方法会发送 JSON body。

## Token 统计

`设置 -> 统计` 会展示累计 token、prompt/completion 拆分、请求数、工具调用数、按日期统计、按模型统计和最近请求。非流式工具决策和流式最终回复的 usage 都会尽量合并记录；如果某个 OpenAI 兼容服务不返回 usage，该次请求不会写入 token 统计。

## 打包命令

### 开发与构建

```powershell
# 安装依赖
npm install

# 前端开发（Vite dev server）
npm run dev

# 前端构建
npm run build

# Tauri 开发模式（同时启动 Vite + Rust 后端）
npm run tauri:dev

# Rust 类型检查（仅检查不编译）
cargo check --manifest-path src-tauri/Cargo.toml

# Rust 测试
cargo test --manifest-path src-tauri/Cargo.toml
```

### 生成发布产物

```powershell
# 完整 Tauri 打包（前端构建 + Rust release 编译 + 打包）
npm run tauri:build
```

打包后会生成：

| 产物 | 路径 |
|------|------|
| 可执行文件 | `src-tauri/target/release/ipet.exe` |
| MSI 安装包 | `src-tauri/target/release/bundle/msi/iPet_0.1.0_x64_en-US.msi` |
| Portable zip | `src-tauri/target/release/bundle/zip/iPet_0.1.0_x64_en-US.zip` |

### 已知问题：WiX MSI ICE 校验

当前机器上 `npm run tauri:build` 可能在 WiX 生成 MSI 的 ICE 校验阶段失败，这是本机 Windows Installer / WiX 环境问题，不是应用源码错误。

**Workaround**（在 Tauri 已生成 `target/release/wix/x64/main.wixobj` 后手动跳过 ICE 校验）：

```powershell
light.exe -sval -out "src-tauri\target\release\bundle\msi\iPet_0.1.0_x64_en-US.msi" "src-tauri\target\release\wix\x64\main.wixobj"
```

如果 WiX toolset 未安装，需先执行：

```powershell
cargo install cargo-wix
```

## 常用命令

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri:build
```

在当前机器上，Tauri 可以生成 `src-tauri/target/release/ipet.exe`。如果 WiX MSI 打包阶段报 Windows Installer ICE 校验错误，可在 Tauri 已生成 `target/release/wix/x64/main.wixobj` 后使用 WiX `light.exe -sval` 跳过 ICE 校验生成 MSI；这属于本机 Windows Installer 服务环境问题，不是应用源码错误。

## 产物位置

- 可执行文件：`src-tauri/target/release/ipet.exe`
- MSI 安装包：`src-tauri/target/release/bundle/msi/iPet_0.1.0_x64_en-US.msi`
- Portable zip：`src-tauri/target/release/bundle/zip/iPet_0.1.0_x64_en-US.zip`
