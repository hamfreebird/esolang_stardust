# AGENTS.md — esolang_stardust

Rust interpreter + LLVM compiler for the **Stardust** esolang (space-count-encoded instructions), plus a
VSCode extension in `stardust-vscode/`. Docs/comments in this repo are written in **Chinese** — match that
when editing prose or comments; do not translate existing text.

## Commands

The binary is `esolang_stardust` (crate name). `readme.md`/`docs/` call it `stardust`, but `cargo build`
produces `target/{debug,release}/esolang_stardust`. `rust-toolchain.toml` pins channel `1.95` (edition 2024).

```bash
cargo build                                  # target/debug/esolang_stardust
cargo test                                   # 302 lib unit + 99 integration tests, ~1s
cargo test --lib                             # in-module #[cfg(test)] unit tests only
cargo test --test integration_test
cargo test lexer                             # substring filter on module/test name
cargo test codegen_tests::compile_loop_runs  # single test
cargo test -- --skip codegen_tests::compile_toolchain_check   # exactly what CI runs
cargo fmt --all -- --check
cargo clippy --all-targets
./target/debug/esolang_stardust hello_world.sd
./target/debug/esolang_stardust --check hello_world.sd         # JSON diagnostics on stdout
```

Verify real program output through the CLI, not tests (see gotcha 3).

### CI specifics worth knowing before you "fix" CI

`.github/workflows/ci.yml` runs on push/PR to `master`: `cargo build` → `cargo test -- --skip
codegen_tests::compile_toolchain_check` → `cargo clippy --all-targets -- -D warnings` → `cargo fmt --check`,
plus `cd stardust-vscode && npm ci && npm run compile`.

- Clippy is now genuinely enforced at `-D warnings` and the tree is clean. Keep it that way —
  `unwrap_used` is warn in `src/` (allowed in tests via `.clippy.toml`), so prefer `?`/`match` over `unwrap()`.
- `--build` and the `codegen_tests` module need `clang` + `llc` on `PATH`. Some codegen tests *silently
  skip* their runtime assertions when `clang` is missing (`compile_and_run` returns `None`); the
  `*_produces_valid_ir` tests use `validate_ir()` which **panics** if clang rejects the IR, so those are
  the ones that actually guard the compiler. `compile_toolchain_check` is the only test that hard-fails
  on a missing toolchain, hence the CI skip.

## Pipeline & architecture

`src/main.rs::stardust()` dispatches by flag to `cmd_run` (default), `--debug`, `--repl`, `--check`,
`--tokens`, `--compile`, `--build`, `--stardust`, `--dump`.

Run pipeline: read file → `parse_config` (strip `#[key=value]`, replacing each annotation line with a
blank line so error line numbers still match the source) → `simple_preprocess` (ASCII alnum → N spaces
+ `+`) → `tokenize` → `parse_program` → `VM::run` **or** `codegen::compile_to_ir`.

Every CLI mode that touches a `.sd` file goes through `parse_config` — `--check`, `--tokens`,
`--compile`, `--build` and `--dump` strip annotations too. Adding a new mode must do the same, or files
using `#[max_steps=…]` will fail to lex (`invalid character '['`).

- `Instruction` in `src/stardust/mod.rs` is the source of truth for the instruction set (36 variants);
  each variant carries `InstrMeta` (`SourceSpan`) used for error locations and debugger stepping.
- `src/stardust/lexer.rs` is space-counting/symbol-agnostic: it only produces
  `(spaces, TokenType, line, column, byte_pos)`. Decoding spaces → `Instruction` lives in
  `src/stardust/parser.rs` (which also validates marks/duplicate functions at parse time).
- VM is multi-frame: `frames[0]` is main; a call pushes a `CallFrame` with its own `Vec<i64>` stack
  (`Rc` shared instruction body). On return the callee's **entire stack is appended to the parent**
  (multi-value return) and `parent.pc = ret_pc`. Heap is global `HashMap<i64, i64>` shared by all frames.
- Codegen (`src/codegen/`) is pure-stdlib LLVM IR *text* generation — no LLVM crates. Order:
  `optimizer` (3-phase fixed point: const-fold → DCE → peephole) → `compiler` (compile-time stack
  tracking into SSA regs, flushed to memory only at control-flow boundaries) → `.ll` → `llc` → `clang`.
- `VmIO` trait in `src/stardust/vm.rs` abstracts I/O; `VM::run()` uses `StdIO`, `VM::run_with_io()`
  accepts an injected implementation.

## Gotchas that will bite you

1. `--check` exits **0** even when it reports errors — the result lives in the JSON `status` field
   (the VSCode extension parses stdout). Never gate tooling on `--check`'s exit code.
2. Most integration tests run `vm.run()` with `StdIO` and assert `is_ok()` / `ErrorKind`, so they do
   **not** capture stdout. Use the `TestIO` stub in `tests/integration_test.rs` + `run_with_io` when you
   need output assertions.
3. `stardust-vscode/out/extension.js` is **committed build output**. After editing `src/extension.ts`,
   run `cd stardust-vscode && npm run compile` and commit the regenerated `out/`.
4. `hello_world.sd` is `include_str!`-ed by two tests; renaming/deleting it breaks the build.
5. `.cargo/` and `build/` are gitignored — the cross-compile linkers in `.cargo/config.toml`
   (mingw/aarch64) and any prebuilt binaries/`.vsix` are local-only, not CI-visible. `*.ll` is ignored too.
6. `simple_preprocess` rewrites **every** ASCII alphanumeric in the source into `Push(n)` — including
   digits inside what you meant as function names. Write `   : ;` (spaces), never `3:;`.
7. In `--stardust` mode *all* printable ASCII (including the 15 symbol characters) is converted to
   `Push+CharOut`; the old behaviour kept symbols verbatim, which produced unlexable output.
8. rustfmt uses `max_width = 100` and `use_small_heuristics = "Max"` (statements pack onto one line).
   Run `cargo fmt`; default rustfmt settings would churn the whole tree.

## Compiler invariants (src/codegen/)

The IR backend emits LLVM IR **as text**, so two classes of bug are silent until `clang` runs:

- **SSA names must be unique per function body.** Deriving a value in two steps (e.g. `len-1`, then
  `len-1-j`) needs two distinct names. Labels also need to not collide with the `i1` variables of the
  same name — use a `_blk` suffix for labels.
- **Every basic block must end with a terminator** (`br`/`ret`/`switch`/…). `FuncCompiler::emitln`
  tracks this in `block_terminated`; always open a new block with `emit_label()`, never `emitln` a raw
  `foo:` line. Constant strings must declare `[N x i8]` with the exact byte length, and the matching
  `getelementptr` must use the same `N`.
- `pop_ssa` falls back to loading from `@stack` at basic-block starts; it now guards against
  stack underflow via the frame base so it can't read uninitialised memory.

`tests/integration_test.rs::every_single_instruction_produces_valid_ir` compiles a minimal program for
each instruction and validates the IR with clang — add cases there when you touch codegen.

## Adding or changing an instruction

Traced from an existing one (`Xor`), these are all the places that must change together:

- `src/stardust/mod.rs` — enum variant + `Instruction::format()`
- `src/stardust/parser.rs` — spaces → instruction mapping (add/update `ErrorKind::InvalidSpacesFor*` in
  `src/stardust/mod.rs` if the valid space counts for that symbol change)
- `src/stardust/vm.rs` — `execute_in_frame` (exhaustive match)
- `src/codegen/compiler.rs` — `compile_instruction` (exhaustive match; codegen won't compile otherwise).
  Read the "Compiler invariants" section above before touching this file.
- `src/repl/parser.rs` — REPL shorthand name, plus the `for name in [...]` help list if applicable
- `src/stardust/debugger.rs` — `inst_meta()` arm
- optional: `src/codegen/optimizer.rs` const-folding, `stardust-vscode/src/extension.ts` hover text,
  `stardust-vscode/syntaxes/sd.tmLanguage.json` highlighting
- docs: `docs/language/instructions.md` and the instruction tables in `readme.md`
- tests: add a case to `tests/integration_test.rs::every_single_instruction_produces_valid_ir`

Adding a *new symbol character* (beyond the 16 in `TokenType`) additionally needs lexer changes
(`is_symbol`, `char_to_token_type`).

Recursion caveat: a function body calling **itself** (`   : ;`) hits the same code path as the
declaration terminator, so `parse_colon_at_top_level` checks `is_call_after_colon()` before matching the
closing colon. Don't "simplify" that check away.

## Docs

`docs/` (14 files, index at `docs/README.md`) is thorough and Chinese; `docs/file-index.md` is a
per-file map, `docs/implementation/pipeline.md` and `docs/implementation/vm.md` cover the real
execution flow. Docs drift from code — e.g. `docs/README.md` still states MSRV 1.85 while `Cargo.toml`
says `rust-version = "1.95"`, and `docs/file-index.md` references files like `IMPROVEMENTS.md` that no
longer exist. Trust `Cargo.toml`, CI config, and the source over prose.

`readme.md` examples are pinned by `tests/integration_test.rs` (`readme_arithmetic_example_runs`),
so changing one means changing the test. Don't paste untested esolang into the readme — the previous
Fibonacci example never ran (the terminator used a digit literal, and self-recursion was unparseable).
