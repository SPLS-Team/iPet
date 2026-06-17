# echo_local

`echo_local` 是一个示例本地工具（`kind: "local"`），用于演示 iPet 的子进程 stdio
工具协议。它把传入的 `text` 参数原样回显。

## 文件

```text
echo_local/
├── tool.json       # schemaVersion 2, kind=local
└── echo_tool.js    # 随包分发的 Node 脚本
```

## 调用约定

- iPet spawn `node echo_tool.js`（cwd 为包目录）。
- 模型参数作为一行 JSON 写到子进程 stdin，如 `{"text":"hello"}\n`。
- 子进程把结果写到 stdout：`{"echo":"hello","length":5}`。
- 超时 15 秒、stdout 上限 2 MiB、非零退出码即报错。

## 导入

```
设置 → 工具 tab → 导入工具包 → 路径填本目录
```

或在 `docs/TOOL_PACKAGE.md` 查看完整协议。

## 运行前提

宿主机器需安装 Node.js（`node` 在 PATH 中）。若改用 Python/其它运行时，把
`tool.json` 的 `command`/`args` 换成对应的解释器与脚本即可。
