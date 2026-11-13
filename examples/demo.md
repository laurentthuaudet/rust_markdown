# Rust Markdown — démo

Un document pour tester le **rendu**, l'*édition* et les ~~exports~~.

## Liste de tâches

- [x] Visualiser du Markdown
- [x] Éditer avec aperçu côte à côte
- [ ] Exporter en PDF, HTML, DOCX, EPUB

## Tableau

| Format | Moteur        | Statut |
|:-------|:--------------|-------:|
| HTML   | comrak        |     ✅ |
| PDF    | typst         |     🚧 |

## Code

```rust
fn main() {
    println!("Bonjour, Markdown !");
}
```

> [!NOTE]
> Les liens externes comme https://www.rust-lang.org s'ouvrent dans le navigateur.

Une note de bas de page[^1].

[^1]: Rendu via les extensions GFM.
