# Project Picker

Fast fuzzy TUI project picker built with Rust, Ratatui, and Nucleo.

## Build & Install

**After every code change, always run:**

```sh
just reinstall
```

This lints, format-checks, builds a release binary, and installs it to `~/.cargo/bin/project-picker`.

## Stack

- **Language:** Rust (edition 2021)
- **TUI:** Ratatui + Crossterm
- **Fuzzy matching:** Nucleo
- **CLI args:** Clap (derive)

## Project structure

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI arg parsing, action dispatch (print/cd/code/run) |
| `src/project.rs` | `Project` struct, JSON cache loading |
| `src/ui.rs` | TUI layout, rendering, input handling |

## Key commands

| Command | What it does |
|---------|-------------|
| `just install` | Build release + install binary |
| `just reinstall` | Lint + fmt-check + install (use this one) |
| `just check` | Lint + fmt-check + debug build |
| `just lint` | Clippy |
| `just fmt` | Format code |
