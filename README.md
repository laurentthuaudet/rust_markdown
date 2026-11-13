# rust_markdown

A desktop Markdown reader/editor for Windows, written in Rust with [Dioxus](https://dioxuslabs.com).

## Features (planned)

- View Markdown (CommonMark + GFM: tables, task lists, strikethrough, autolinks, footnotes) with syntax-highlighted code blocks
- Edit with a side-by-side live preview
- Export to HTML, PDF, DOCX and EPUB (pure Rust, no external tools)
- **Environments**: group Markdown files scattered across the disk into named collections
  (a file can belong to several environments), tag them, and find them again by name,
  tag, modification date or full-text search (SQLite + FTS5)

## Development workflow

- Each feature lives on its own branch (`feat/...`, `chore/...`) and is merged into `main` with `--no-ff`
- Commits follow [Conventional Commits](https://www.conventionalcommits.org)
