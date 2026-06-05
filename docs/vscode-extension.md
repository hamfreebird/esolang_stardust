# VSCode 扩展

> **实现位置**：`stardust-vscode/` · TypeScript 实现

## 1. 安装

从 `build/stardust-0.1.5.vsix` 安装，或从源码构建：

```bash
cd stardust-vscode
npm install
npx vsce package
```

## 2. 功能

### 2.1 语法高亮

**实现**：`syntaxes/sd.tmLanguage.json` — TextMate 语法定义

为 `.sd` 和 `.stardust` 文件提供基于符号类型的着色：

- 16 种指令符号按语义分组着色（Push/算术/控制流/函数/I/O/比较/逻辑/堆/栈扩展/调试）
- 前导空格可视化（不明显但保持缩进感觉）
- 注释 `//` 以不同颜色显示
- 运行时注解 `#[key=value]` 高亮

**语言配置**（`language-configuration.json`）：
- 行注释：`//`
- 括号匹配：无（Stardust 不使用括号）

### 2.2 悬停提示

鼠标悬停在符号上时显示：
- 指令名称
- 栈变化描述
- 前导空格数量含义

**已知限制**：`=`, `&`, `-`, `<`, `>`, `#` 六组符号的提示信息未随 v0.1.5 指令集扩展更新。

### 2.3 实时诊断

保存文件时自动运行 `stardust --check`，解析 JSON 输出并标注错误到对应行：

```typescript
// extension.ts: diagnostics collection
const diagnostics = vscode.languages.createDiagnosticCollection('stardust');

// On save:
const result = execSync(`stardust --check ${filePath}`);
const parsed = JSON.parse(result);
for (const diag of parsed.diagnostics) {
    const range = new vscode.Range(diag.line - 1, diag.column - 1,
                                   diag.line - 1, diag.column);
    diagnostics.set(uri, [new vscode.Diagnostic(range, diag.message,
                         vscode.DiagnosticSeverity.Error)]);
}
```

### 2.4 命令

| 命令 | 快捷键 | 说明 |
|------|--------|------|
| `stardust.run` | `F5` | 在集成终端中运行当前文件 |
| `stardust.check` | `Ctrl+Shift+C` | 手动触发语法检查 |
| `stardust.compile` | — | 编译当前文件为 LLVM IR |

## 3. 扩展配置

| 配置键 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `stardust.cliPath` | string | `"stardust"` | stardust CLI 二进制路径（支持绝对路径） |
| `stardust.runInTerminal` | boolean | `true` | 使用集成终端运行（false 则使用 output channel） |
| `stardust.checkOnSave` | boolean | `true` | 保存文件时自动运行语法检查 |

## 4. 文件结构

```
stardust-vscode/
├── package.json                       # 扩展清单（名称、版本、激活事件、配置项）
├── tsconfig.json                      # TypeScript 编译配置
├── language-configuration.json        # 语言配置（注释、括号）
├── syntaxes/
│   └── sd.tmLanguage.json             # TextMate 语法高亮定义
├── src/
│   └── extension.ts                   # 扩展入口（激活、诊断、命令、悬停）
├── out/
│   ├── extension.js                   # 编译后的扩展 JS
│   └── extension.js.map               # Source map
└── node_modules/                      # npm 依赖
```

## 5. package.json 关键配置

```json
{
  "name": "stardust",
  "displayName": "Stardust Language Support",
  "version": "0.1.5",
  "engines": { "vscode": "^1.85.0" },
  "activationEvents": ["onLanguage:stardust"],
  "contributes": {
    "languages": [{ "id": "stardust", "extensions": [".sd", ".stardust"] }],
    "grammars": [{ "language": "stardust", "path": "./syntaxes/sd.tmLanguage.json" }],
    "commands": [
      { "command": "stardust.run", "title": "Run Stardust File" },
      { "command": "stardust.check", "title": "Check Stardust Syntax" }
    ],
    "keybindings": [
      { "command": "stardust.check", "key": "ctrl+shift+c", "when": "editorLangId == stardust" }
    ],
    "configuration": {
      "properties": {
        "stardust.cliPath": { "type": "string", "default": "stardust" },
        "stardust.runInTerminal": { "type": "boolean", "default": true },
        "stardust.checkOnSave": { "type": "boolean", "default": true }
      }
    }
  }
}
```

---

> 相关文件：[cli.md](cli.md) · [errors.md](errors.md)
