# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build/Test/Lint

```bash
cargo build                  # Debug build
cargo run -- <args>          # Run the interpreter
cargo test                   # Run all tests (unit tests are inline in source files)
```

No linter or formatter is configured. The project uses Rust edition 2024.

## Architecture

Stardust is an esolang interpreter — a whitespace-based language where instruction semantics are determined by the number of leading spaces before a symbol character. The interpreter is a classic compiler pipeline implemented in Rust.

**Processing pipeline** (see `src/main.rs:32-132`):

1. **Preprocess** (`src/extension/unwind.rs`) — Converts visible ASCII characters in source to Stardust push-char instructions. Two modes: `simple_preprocess` (direct ASCII→push mapping) and `preprocess` (uses hand-crafted pretty Stardust snippets from `char2sd_list.rs`).
2. **Lex** (`src/stardust/lexer.rs`) — `Lexer` struct tokenizes space counts + symbol chars into `Token` structs. Comments (`//`) are consumed during lexing and produce no tokens.
3. **Parse** (`src/stardust/parser.rs`) — `Parser` struct converts a `Vec<Token>` into `ParseResult` containing `main_instructions: Vec<Instruction>`, `main_marks: HashMap<usize, usize>`, and `functions: HashMap<usize, Vec<Instruction>>`. Function declarations are parsed as delimited blocks between matching `(n):` tokens. Nested function calls inside function bodies are forbidden (`CallInsideFunction` error).
4. **Execute** (`src/stardust/vm.rs`) — `VM` struct runs instructions against `main_stack: Vec<i64>`. Function calls create a new local stack, execute the function body, then merge the remaining stack back into the main stack (multi-value return).

**Core types** are defined in `src/stardust/mod.rs`:
- `Token` — spaces count + token type + source position
- `Instruction` enum — all VM operations (Push, Dup, Add, Jump, Call, etc.)
- `ParseResult` — output of the parser
- `VM` struct — runtime state (stack, PC, instructions, marks, functions)
- `ErrorKind` / `StardustError` — all error variants with source spans

**CLI modes** (`src/main.rs`):
- `stardust <file.sd>` — run a program (default)
- `stardust --stardust <input> [output]` — convert text file to Stardust source
- `stardust --dump <file> [output]` — analyze and dump all pipeline stages
- `stardust` (no args) — launches the Pygame-based IDE (`src/ide_py/ide.py`)

The IDE is a separate Python program using Pygame that visualizes Stardust source code with syntax highlighting and instruction tooltips.

**Key design rules**:
- Instruction semantics are determined by leading space count + symbol: e.g., `0*` = Add, `1*` = Sub, `5+` = Push(0), `1+` = Dup
- Mark names and function names are both identified by space count (usize), and the same numeric value can be used for both a mark and a function name
- The VM forbids `Call` instructions inside function bodies — functions cannot call other functions
- Source files use `.stardust` or `.sd` extension
