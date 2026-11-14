# rust_markdown

A desktop Markdown reader/editor for Windows, written in Rust with [Dioxus](https://dioxuslabs.com).

## Features

- **View** Markdown: CommonMark + GitHub Flavored Markdown (tables, task lists,
  strikethrough, autolinks, footnotes, alerts) with syntax-highlighted code blocks
  and local images
- **Edit** in a plain-text editor with a live side-by-side preview
  (editor only / side by side / preview only); both panes scroll together
- Line numbers (toggle from the right-click menu), find & replace, light / dark /
  system theme; footnotes stay where they are written and in-document links
  (footnotes, anchors) scroll the preview
- Safe editing: closing the window asks about unsaved changes; files changed by
  other programs are reloaded (or you are asked, if you have local edits) and the
  library index follows them live
- **Export** to HTML (self-contained), PDF (Typst engine), Word (DOCX) and EPUB 3 —
  pure Rust, no external tool required
- **Environments**: group Markdown files scattered across the disk into named
  collections. A file can belong to several environments. Tag files and find them
  again by name/title, tags, modification date or full-text search (accent-insensitive)

## Usage

| Action | How |
|--------|-----|
| New / open / save / save as | Toolbar or `Ctrl+N`, `Ctrl+O`, `Ctrl+S`, `Ctrl+Shift+S` |
| Open a file at startup | `rust-markdown path\to\file.md` (works with "Open with…") |
| Export | Toolbar → **Exporter** → format |
| Create an environment | Library sidebar → **+** |
| Add files to an environment | Select it → **Ajouter des fichiers** / **Ajouter un dossier**, or use **Ajouter à…** in the document bar |
| Tag the open document | Document bar → **Tags** (comma-separated, `Enter` to apply) |
| Filter | Library sidebar → full text, name, date range, tags |
| Find / replace | `Ctrl+F` / `Ctrl+H` (or right-click in the editor), `Enter` / `Shift+Enter` to move, `Échap` to close |
| Line numbers | Right-click in the editor → **Numéros de ligne** |
| Theme | Toolbar → **◐ Système / ☀ Clair / ☾ Sombre** |

Preferences (theme, line numbers) are saved in `%APPDATA%\rust_markdown\settings.json`.

The library index lives in `%APPDATA%\rust_markdown\library.db`. It only stores
references: files are never moved or modified by the library. Files changed
outside the application are reindexed as soon as they change.

## Workspace layout

| Crate | Role |
|-------|------|
| `crates/rmd-core` | Markdown parsing and HTML rendering (comrak + syntect) |
| `crates/rmd-export` | Export to HTML, PDF (typst), DOCX (docx-rs), EPUB (epub-builder) |
| `crates/rmd-store` | Environments, tags and full-text index (SQLite + FTS5) |
| `crates/rmd-app` | Dioxus desktop application (binary `rust-markdown`) |

## Build & run

Requirements: Rust (stable, MSVC toolchain), Visual Studio Build Tools (C++
workload) and the WebView2 runtime (included in Windows 11).

```bash
cargo run -p rust-markdown --release
```

Run the tests:

```bash
cargo test --workspace
```

## Installer

A Windows installer (`setup.exe`, per-user, no admin rights) is built with
[cargo-packager](https://github.com/crabnebula-dev/cargo-packager):

```bash
cargo install cargo-packager --locked
```

```bash
cargo packager --release -p rust-markdown
```

The installer lands in `dist/`. It adds Start menu (and optionally desktop)
shortcuts, an uninstaller, and registers Rust Markdown for `.md`, `.markdown`,
`.mdown` and `.mkd` files ("Open with"). Pushing a `v*` tag builds it in CI and
attaches it to a GitHub release.

The application icon comes from the Caretta design system
(`crates/rmd-app/assets/icon.svg`); `build.rs` renders it into the executable.

## Command-line tools

Try an exporter from the command line:

```bash
cargo run -p rmd-export --example export -- examples/demo.md demo.pdf
```

## Development workflow

- Each feature lives on its own branch (`feat/...`, `chore/...`) and is merged into `main` with `--no-ff`
- Commits follow [Conventional Commits](https://www.conventionalcommits.org)
- CI (GitHub Actions, Windows) runs `cargo fmt --check`, `clippy -D warnings` and the tests

Ideas not yet validated are tracked in [docs/BACKLOG.md](docs/BACKLOG.md).
