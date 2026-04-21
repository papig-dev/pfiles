# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build          # debug build
cargo run            # build and run
cargo build --release
cargo check          # fast type-check without linking
```

No tests or linter config exist yet.

## What this is

`pfiles` is a dual-pane desktop file manager built with `egui/eframe`. It is a prototype that reuses the core Rust logic originally written for `../rift` (a Tauri-based file manager), stripped of all Tauri annotations so it can be called directly from `egui`.

## Architecture

All UI lives in `src/app.rs` as a single `PFilesApp` struct implementing `eframe::App`. There is no separate component system — each logical section is a `render_*` method.

### UI layout (rendered every frame in `update()`)

```
TopBottomPanel (toolbar)   ← render_toolbar()
SidePanel::right (preview) ← render_preview_panel()   [only when show_preview]
CentralPanel               ← two panes via ui.columns(2, ...)
  Left pane                ← render_pane(Side::Left)
  Right pane               ← render_pane(Side::Right)
TopBottomPanel (status)
Window (modal)             ← render_modal()
```

### State

`PFilesApp` holds two `PaneState` structs (`left`, `right`) plus shared state:
- `active: Side` — which pane has keyboard focus
- `show_preview / preview_data / preview_texture` — right panel preview state
- `archive_entries / show_archive_panel` — archive listing (shown in preview panel)
- `modal: Option<ModalState>` — Rename / Mkdir / ConfirmDelete

### Core modules (`src/core/`)

| Module | Purpose |
|--------|---------|
| `fs_ops` | `list_dir`, copy/move/delete/rename, `FileEntry` struct |
| `archive` | List entries inside zip/7z/tar/tar.gz/gz archives |
| `preview` | Read file and return `PreviewData` (Text / Image / Binary) |

`fs_ops::list_dir` sorts dirs before files, then alphabetically. Hidden files are filtered by `is_hidden` (name starts with `.`).

### Key design decisions

- **Tab key**: removed from egui's raw event queue in `handle_shortcuts` so egui's focus traversal cannot also see it. After switching panes, widget focus is explicitly surrendered so arrow keys work on the file list immediately.
- **Korean filenames**: macOS HFS+/APFS stores filenames as NFD. All displayed names are NFC-normalized via `unicode_normalization`.
- **File list columns**: rendered with `egui_extras::TableBuilder` (Type | Name | Size | Modified). Column widths are resizable.
- **Font loading**: `configure_fonts()` tries bundled `assets/fonts/JetBrainsMonoHangul*` first, then falls back to system Korean fonts. If none found, egui's default font is used (no Korean support).
- **Preview**: single "Preview" toggle button — opening it also loads the selected file. Closing it just hides the panel.

### Adding a new file operation

1. Add the function to `src/core/fs_ops.rs`.
2. Call it from `PFilesApp` in `src/app.rs` (typically triggered by a keyboard shortcut in `handle_shortcuts` or a toolbar button in `render_toolbar`).
3. Reload the affected pane(s) with `self.reload(side)` after the operation.
