import * as vscode from 'vscode';
import { execFile } from 'child_process';

// ---------------------------------------------------------------------------
// 配置
// ---------------------------------------------------------------------------

function getCliPath(): string {
    return vscode.workspace.getConfiguration('stardust').get<string>('cliPath') || 'stardust';
}

// ---------------------------------------------------------------------------
// 诊断：保存时自动检查语法
// ---------------------------------------------------------------------------

interface JsonDiagnostic {
    severity: string;
    line: number;
    column: number;
    message: string;
    code: string;
}

interface CheckResult {
    status: string;
    diagnostics: JsonDiagnostic[];
}

const diagnosticCollection =
    vscode.languages.createDiagnosticCollection('stardust');

async function runCheck(document: vscode.TextDocument): Promise<void> {
    const cliPath = getCliPath();

    return new Promise((resolve) => {
        execFile(cliPath, ['--check', document.fileName], { timeout: 10000 }, (error, stdout, stderr) => {
            if (error && !stdout) {
                // CLI 崩溃或找不到
                diagnosticCollection.clear();
                resolve();
                return;
            }

            try {
                const result: CheckResult = JSON.parse(stdout);
                const diagnostics: vscode.Diagnostic[] = [];

                for (const d of result.diagnostics) {
                    const line = Math.max(0, (d.line || 1) - 1);
                    const col = Math.max(0, (d.column || 1) - 1);
                    const range = new vscode.Range(line, col, line, col + 1);

                    const diagnostic = new vscode.Diagnostic(
                        range,
                        d.message,
                        d.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning
                    );
                    diagnostic.source = 'stardust';
                    diagnostic.code = d.code;
                    diagnostics.push(diagnostic);
                }

                diagnosticCollection.set(document.uri, diagnostics);
            } catch {
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
function getInstructionHint(spaces: number, symbol: string): string | null {
    switch (symbol) {
        case '+':
            if (spaces === 0) return null; // 无效
            if (spaces === 1) return '**Dup** — 复制栈顶元素';
            if (spaces === 2) return '**Swap** — 交换栈顶两个元素';
            if (spaces === 3) return '**Rotate** — 旋转栈顶三个元素\n`[a, b, c] → [c, a, b]`';
            if (spaces === 4) return '**Pop** — 弹出栈顶元素';
            // spaces >= 5
            return `**Push(${spaces - 5})** — 将值 ${spaces - 5} 压入栈中`;

        case '*':
            if (spaces === 0) return '**Add** — 弹出两个值，压入其和\n`a b → (a + b)`';
            if (spaces === 1) return '**Sub** — 弹出两个值，压入其差\n`a b → (a - b)`';
            if (spaces === 2) return '**Mul** — 弹出两个值，压入其积\n`a b → (a * b)`';
            if (spaces === 3) return '**Div** — 弹出两个值，压入其商\n`a b → (a / b)`';
            if (spaces === 4) return '**Mod** — 弹出两个值，压入其余数\n`a b → (a % b)`';
            if (spaces === 5) return '**Reverse** — 反转整个栈';

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
            if (spaces === 0) return '**NumOut** — 弹出栈顶并以数字输出';
            if (spaces === 1) return '**NumIn** — 从标准输入读取数字并压栈';
        case ',':
            if (spaces === 0) return '**CharOut** — 弹出栈顶并以 ASCII 字符输出';
            if (spaces === 1) return '**CharIn** — 从标准输入读取一个字符并压栈';

        case '-':
            return `**比较/赋值符号** (空格数=${spaces}) — 保留关键字`;
        case '=':
            return `**比较/赋值符号** (空格数=${spaces}) — 保留关键字`;
        case '<':
            return `**比较/赋值符号** (空格数=${spaces}) — 保留关键字`;
        case '>':
            return `**比较/赋值符号** (空格数=${spaces}) — 保留关键字`;
        case '&':
            return `**逻辑符号** (空格数=${spaces}) — 保留关键字`;
        case '#':
            return `**调试符号** (空格数=${spaces}) — 保留关键字`;
    }
    return null;
}

/**
 * 计算给定列之前连续空格的数量。
 */
function countLeadingSpaces(line: string, col: number): number {
    // col 是符号位置，向左数连续空格
    let count = 0;
    for (let i = col - 1; i >= 0 && line[i] === ' '; i--) {
        count++;
    }
    return count;
}

class StardustHoverProvider implements vscode.HoverProvider {
    provideHover(
        document: vscode.TextDocument,
        position: vscode.Position
    ): vscode.Hover | null {
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

async function runCurrentFile(): Promise<void> {
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
    const useTerminal = vscode.workspace.getConfiguration('stardust').get<boolean>('runInTerminal') ?? true;

    if (useTerminal) {
        // 在集成终端中运行
        const terminal = vscode.window.activeTerminal || vscode.window.createTerminal('Stardust');
        terminal.show();
        terminal.sendText(`${cliPath} "${document.fileName}"`);
    } else {
        // 在 Output 面板中运行
        const outputChannel = vscode.window.createOutputChannel('Stardust Output');
        outputChannel.clear();
        outputChannel.show();

        execFile(cliPath, [document.fileName], { timeout: 30000 }, (error, stdout, stderr) => {
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

async function checkCurrentFile(): Promise<void> {
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
    } else {
        vscode.window.showWarningMessage(`Stardust: ${diagnostics.length} issue(s) found`);
    }
}

// ---------------------------------------------------------------------------
// 扩展激活
// ---------------------------------------------------------------------------

export function activate(context: vscode.ExtensionContext): void {
    // 注册 Hover Provider
    context.subscriptions.push(
        vscode.languages.registerHoverProvider('sd', new StardustHoverProvider())
    );

    // 注册命令
    context.subscriptions.push(
        vscode.commands.registerCommand('stardust.run', runCurrentFile)
    );
    context.subscriptions.push(
        vscode.commands.registerCommand('stardust.check', checkCurrentFile)
    );
    context.subscriptions.push(
        vscode.commands.registerCommand('stardust.compile', async () => {
            vscode.window.showInformationMessage('Stardust: Use "stardust --stardust <input> [output]" from the terminal.');
        })
    );

    // 注册诊断集合
    context.subscriptions.push(diagnosticCollection);

    // 保存时自动检查
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument((doc) => {
            const checkOnSave = vscode.workspace.getConfiguration('stardust').get<boolean>('checkOnSave') ?? true;
            if (doc.languageId === 'sd' && checkOnSave) {
                runCheck(doc);
            }
        })
    );

    // 打开文件时检查
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor((editor) => {
            if (editor && editor.document.languageId === 'sd') {
                const checkOnSave = vscode.workspace.getConfiguration('stardust').get<boolean>('checkOnSave') ?? true;
                if (checkOnSave) {
                    runCheck(editor.document);
                }
            }
        })
    );

    // 初始检查
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'sd') {
        runCheck(editor.document);
    }
}

export function deactivate(): void {
    diagnosticCollection.clear();
}
