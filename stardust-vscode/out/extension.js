"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const child_process_1 = require("child_process");
// ---------------------------------------------------------------------------
// 配置
// ---------------------------------------------------------------------------
function getCliPath() {
    return vscode.workspace.getConfiguration('stardust').get('cliPath') || 'stardust';
}
const diagnosticCollection = vscode.languages.createDiagnosticCollection('stardust');
async function runCheck(document) {
    const cliPath = getCliPath();
    return new Promise((resolve) => {
        (0, child_process_1.execFile)(cliPath, ['--check', document.fileName], { timeout: 10000 }, (error, stdout, stderr) => {
            if (error && !stdout) {
                // CLI 崩溃或找不到
                diagnosticCollection.clear();
                resolve();
                return;
            }
            try {
                const result = JSON.parse(stdout);
                const diagnostics = [];
                for (const d of result.diagnostics) {
                    const line = Math.max(0, (d.line || 1) - 1);
                    const col = Math.max(0, (d.column || 1) - 1);
                    const range = new vscode.Range(line, col, line, col + 1);
                    const diagnostic = new vscode.Diagnostic(range, d.message, d.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning);
                    diagnostic.source = 'stardust';
                    diagnostic.code = d.code;
                    diagnostics.push(diagnostic);
                }
                diagnosticCollection.set(document.uri, diagnostics);
            }
            catch {
                diagnosticCollection.clear();
            }
            resolve();
        });
    });
}
// ---------------------------------------------------------------------------
// 悬停提示
// ---------------------------------------------------------------------------
/**
 * 根据前导空格数和符号字符返回指令说明。
 * 逻辑与 Rust 解释器保持一致。
 */
function getInstructionHint(spaces, symbol) {
    switch (symbol) {
        case '+':
            if (spaces === 0)
                return null; // 无效
            if (spaces === 1)
                return '**Dup** — 复制栈顶元素';
            if (spaces === 2)
                return '**Swap** — 交换栈顶两个元素';
            if (spaces === 3)
                return '**Rotate** — 旋转栈顶三个元素\n`[a, b, c] → [c, a, b]`';
            if (spaces === 4)
                return '**Pop** — 弹出栈顶元素';
            // spaces >= 5
            return `**Push(${spaces - 5})** — 将值 ${spaces - 5} 压入栈中`;
        case '*':
            if (spaces === 0)
                return '**Add** — 弹出两个值，压入其和\n`a b → (a + b)`';
            if (spaces === 1)
                return '**Sub** — 弹出两个值，压入其差\n`a b → (a - b)`';
            if (spaces === 2)
                return '**Mul** — 弹出两个值，压入其积\n`a b → (a * b)`';
            if (spaces === 3)
                return '**Div** — 弹出两个值，压入其商\n`a b → (a / b)`';
            if (spaces === 4)
                return '**Mod** — 弹出两个值，压入其余数\n`a b → (a % b)`';
            if (spaces === 5)
                return '**Reverse** — 反转整个栈';
        case '`':
            return `**Mark(${spaces})** — 声明跳转标记 ${spaces}`;
        case "'":
            return `**Jump(${spaces})** — 弹出栈顶，非零则跳转到 Mark(${spaces})`;
        case '~':
            return `**UnconditionalJump(${spaces})** — 无条件跳转到 Mark(${spaces})`;
        case ':':
            return `**函数名(${spaces})** — 声明或调用函数 ${spaces}`;
        case ';':
            return `**函数调用参数计数(${spaces})** — 期望 ${spaces} 个参数`;
        case '.':
            if (spaces === 0)
                return '**NumOut** — 弹出栈顶并以数字输出';
            if (spaces === 1)
                return '**NumIn** — 从标准输入读取数字并压栈';
        case ',':
            if (spaces === 0)
                return '**CharOut** — 弹出栈顶并以 ASCII 字符输出';
            if (spaces === 1)
                return '**CharIn** — 从标准输入读取一个字符并压栈';
        // ── 堆操作（符号 -）───────────────────────────────
        case '-':
            if (spaces === 0)
                return '**Store** — 弹出 addr 和 val，写入堆\n`heap[addr] = val`';
            if (spaces === 1)
                return '**Load** — 弹出 addr，从堆读取并压栈\n`push heap[addr]`';
            return `**堆操作** (空格数=${spaces}) — 无效（仅支持 0 或 1）`;
        // ── 比较运算（符号 =）───────────────────────────────
        case '=':
            if (spaces === 0)
                return '**Eq** — 弹出两个值，相等压 1 否则 0\n`a b → (a == b)`';
            if (spaces === 1)
                return '**Ne** — 弹出两个值，不等压 1 否则 0\n`a b → (a != b)`';
            if (spaces === 2)
                return '**Lt** — 弹出两个值，小于压 1 否则 0\n`a b → (a < b)`';
            if (spaces === 3)
                return '**Gt** — 弹出两个值，大于压 1 否则 0\n`a b → (a > b)`';
            if (spaces === 4)
                return '**Le** — 弹出两个值，小于等于压 1 否则 0\n`a b → (a <= b)`';
            if (spaces === 5)
                return '**Ge** — 弹出两个值，大于等于压 1 否则 0\n`a b → (a >= b)`';
            return `**比较运算** (空格数=${spaces}) — 无效（仅支持 0-5）`;
        // ── 栈扩展（符号 < >）───────────────────────────────
        case '<':
            if (spaces === 0)
                return '**ShiftL** — 循环左移\n`[a1..aN] → [a2..aN, a1]`';
            if (spaces === 1)
                return '**Depth** — 压入当前栈深度';
            if (spaces === 2)
                return '**Pick** — 弹出 n，从栈底方向复制第 n 个元素\n`push stack[depth-1-n]`';
            return `**栈扩展** (空格数=${spaces}) — 无效（仅支持 0-2）`;
        case '>':
            if (spaces === 0)
                return '**ShiftR** — 循环右移\n`[a1..aN] → [aN, a1..aN-1]`';
            if (spaces === 1)
                return '**DropN** — 弹出 n，丢弃栈顶 n 个元素';
            return `**栈扩展** (空格数=${spaces}) — 无效（仅支持 0-1）`;
        // ── 逻辑运算（符号 &）───────────────────────────────
        case '&':
            if (spaces === 0)
                return '**And** — 弹出两个值，逻辑与\n`a b → (a && b)`';
            if (spaces === 1)
                return '**Or** — 弹出两个值，逻辑或\n`a b → (a || b)`';
            if (spaces === 2)
                return '**Not** — 弹出一个值，逻辑非\n`a → (!a)`';
            if (spaces === 3)
                return '**Xor** — 弹出两个值，逻辑异或\n`a b → (a xor b)`';
            return `**逻辑运算** (空格数=${spaces}) — 无效（仅支持 0-3）`;
        // ── 调试（符号 #）───────────────────────────────────
        case '#':
            if (spaces === 0)
                return '**DumpStack** — 输出整个栈内容到 stderr（调试）';
            if (spaces === 1)
                return '**DumpState** — 输出 PC/栈深度/堆大小到 stderr（调试）';
            if (spaces === 2)
                return '**Breakpoint** — 调试断点（仅 `--debug` 模式暂停）';
            return `**调试指令** (空格数=${spaces}) — 无效（仅支持 0-2）`;
    }
    return null;
}
/**
 * 计算给定列之前连续空格的数量。
 */
function countLeadingSpaces(line, col) {
    // col 是符号位置，向左数连续空格
    let count = 0;
    for (let i = col - 1; i >= 0 && line[i] === ' '; i--) {
        count++;
    }
    return count;
}
class StardustHoverProvider {
    provideHover(document, position) {
        const line = document.lineAt(position.line).text;
        const col = position.character;
        const ch = col < line.length ? line[col] : ' ';
        // 跳过空格和注释
        if (ch === ' ' || ch === '\t') {
            return null;
        }
        // 检测注释行
        const trimmed = line.trimStart();
        if (trimmed.startsWith('//')) {
            return new vscode.Hover('**注释** — 被解释器忽略');
        }
        // 计算前导空格数
        const leadingSpaces = countLeadingSpaces(line, col);
        const hint = getInstructionHint(leadingSpaces, ch);
        if (hint) {
            const spacesInfo = leadingSpaces > 0
                ? `\n\n前导空格: **${leadingSpaces}**`
                : '\n\n(无前导空格)';
            return new vscode.Hover(new vscode.MarkdownString(hint + spacesInfo));
        }
        return null;
    }
}
// ---------------------------------------------------------------------------
// 运行命令
// ---------------------------------------------------------------------------
async function runCurrentFile() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showErrorMessage('No active editor');
        return;
    }
    const document = editor.document;
    if (document.languageId !== 'sd') {
        vscode.window.showWarningMessage('Not a Stardust (.sd) file');
        return;
    }
    // 确保文件已保存
    if (document.isDirty) {
        await document.save();
    }
    const cliPath = getCliPath();
    const useTerminal = vscode.workspace.getConfiguration('stardust').get('runInTerminal') ?? true;
    if (useTerminal) {
        // 在集成终端中运行
        const terminal = vscode.window.activeTerminal || vscode.window.createTerminal('Stardust');
        terminal.show();
        const safePath = document.fileName.replace(/([$`"\\!])/g, '\\$1');
        terminal.sendText(`${cliPath} "${safePath}"`);
    }
    else {
        // 在 Output 面板中运行
        const outputChannel = vscode.window.createOutputChannel('Stardust Output');
        outputChannel.clear();
        outputChannel.show();
        (0, child_process_1.execFile)(cliPath, [document.fileName], { timeout: 30000 }, (error, stdout, stderr) => {
            if (stdout) {
                outputChannel.append(stdout);
            }
            if (stderr) {
                outputChannel.append(stderr);
            }
            if (error && !stdout && !stderr) {
                outputChannel.appendLine(`Error: ${error.message}`);
            }
        });
    }
}
async function checkCurrentFile() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showErrorMessage('No active editor');
        return;
    }
    const document = editor.document;
    if (document.languageId !== 'sd') {
        vscode.window.showWarningMessage('Not a Stardust (.sd) file');
        return;
    }
    if (document.isDirty) {
        await document.save();
    }
    await runCheck(document);
    const diagnostics = diagnosticCollection.get(document.uri);
    if (!diagnostics || diagnostics.length === 0) {
        vscode.window.showInformationMessage('Stardust: Syntax OK');
    }
    else {
        vscode.window.showWarningMessage(`Stardust: ${diagnostics.length} issue(s) found`);
    }
}
// ---------------------------------------------------------------------------
// 扩展激活
// ---------------------------------------------------------------------------
function activate(context) {
    // 注册 Hover Provider
    context.subscriptions.push(vscode.languages.registerHoverProvider('sd', new StardustHoverProvider()));
    // 注册命令
    context.subscriptions.push(vscode.commands.registerCommand('stardust.run', runCurrentFile));
    context.subscriptions.push(vscode.commands.registerCommand('stardust.check', checkCurrentFile));
    context.subscriptions.push(vscode.commands.registerCommand('stardust.compile', async () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            vscode.window.showErrorMessage('No active editor');
            return;
        }
        const document = editor.document;
        if (document.languageId !== 'sd') {
            // 非 .sd 文件 — 当作源文本处理，转换为 Stardust 代码
            if (document.isDirty)
                await document.save();
            const inputFile = document.fileName;
            const outputFile = inputFile.replace(/\.\w+$/, '.stardust');
            const cliPath = getCliPath();
            (0, child_process_1.execFile)(cliPath, ['--stardust', inputFile, outputFile], { timeout: 30000 }, (error, stdout, stderr) => {
                if (error && !stdout) {
                    vscode.window.showErrorMessage(`Stardust: Compilation failed — ${stderr || error.message}`);
                }
                else {
                    vscode.window.showInformationMessage(`Stardust: Generated ${outputFile}`);
                }
            });
            return;
        }
        // .sd 文件 — 编译为 LLVM IR
        if (document.isDirty)
            await document.save();
        const inputFile = document.fileName;
        const outputFile = inputFile.replace(/\.(sd|stardust)$/, '.ll');
        const cliPath = getCliPath();
        (0, child_process_1.execFile)(cliPath, ['--compile', inputFile, outputFile], { timeout: 30000 }, (error, stdout, stderr) => {
            if (error && !stdout) {
                vscode.window.showErrorMessage(`Stardust: Compilation failed — ${stderr || error.message}`);
            }
            else {
                vscode.window.showInformationMessage(`Stardust: LLVM IR generated → ${outputFile}`);
            }
        });
    }));
    context.subscriptions.push(vscode.commands.registerCommand('stardust.debug', async () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            vscode.window.showErrorMessage('No active editor');
            return;
        }
        const document = editor.document;
        if (document.languageId !== 'sd') {
            vscode.window.showWarningMessage('Not a Stardust (.sd) file');
            return;
        }
        if (document.isDirty)
            await document.save();
        const cliPath = getCliPath();
        const terminal = vscode.window.activeTerminal
            || vscode.window.createTerminal('Stardust Debug');
        terminal.show();
        const safePath = document.fileName.replace(/([$`"\\!])/g, '\\$1');
        terminal.sendText(`${cliPath} --debug "${safePath}"`);
    }));
    // 注册诊断集合
    context.subscriptions.push(diagnosticCollection);
    // 保存时自动检查
    context.subscriptions.push(vscode.workspace.onDidSaveTextDocument((doc) => {
        const checkOnSave = vscode.workspace.getConfiguration('stardust').get('checkOnSave') ?? true;
        if (doc.languageId === 'sd' && checkOnSave) {
            runCheck(doc);
        }
    }));
    // 初次打开文件时检查（仅首次，后续仅保存时触发）
    let checkedInitially = false;
    context.subscriptions.push(vscode.window.onDidChangeActiveTextEditor((editor) => {
        if (!checkedInitially && editor && editor.document.languageId === 'sd') {
            checkedInitially = true;
            const checkOnSave = vscode.workspace.getConfiguration('stardust').get('checkOnSave') ?? true;
            if (checkOnSave) {
                runCheck(editor.document);
            }
        }
    }));
    // 初始检查
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'sd') {
        checkedInitially = true;
        runCheck(editor.document);
    }
}
function deactivate() {
    diagnosticCollection.clear();
}
//# sourceMappingURL=extension.js.map