# Backlog — évolutions à valider

Idées identifiées mais **pas encore validées**. Chaque entrée devient une branche
`feat/…` ou `fix/…` une fois acceptée.

Statuts : 🟡 à valider · 🟢 validé (à planifier) · ✅ fait · ❌ non retenu

## Priorité haute

### ✅ Confirmation à la fermeture de la fenêtre
Fermer la fenêtre avec des modifications non enregistrées ne demande rien : le
travail est perdu. C'est le seul vrai risque de perte de données.
- Intercepter la fermeture (`WindowCloseBehaviour` / événement `CloseRequested` de
  Dioxus desktop) et réutiliser la boîte « Enregistrer / Ne pas enregistrer / Annuler ».
- Effort estimé : petit. — **Fait** (`feat/confirm-close`).

### ✅ Détecter les fichiers modifiés hors de l'application
Si le fichier ouvert change sur le disque (autre éditeur, `git pull`…), l'éditeur
garde l'ancienne version et un enregistrement écraserait les changements.
- Surveiller le fichier ouvert (crate `notify`) ; proposer de recharger, ou avertir
  si le document a aussi des modifications locales.
- Mettre à jour l'index de la bibliothèque à la volée au lieu d'attendre ⟳ ou le
  redémarrage.
- Effort estimé : moyen. — **Fait** (`feat/watch-files`).

## Priorité moyenne

### ✅ Images SVG dans l'export DOCX
Word n'accepte que des images matricielles : les SVG sont actuellement ignorés.
- Rasteriser les SVG (crate `resvg`) avant l'insertion.
- Effort estimé : petit à moyen. — **Fait** (`feat/docx-svg-images`) : rendu en PNG 2x avec `resvg`.

### ✅ Installeur, icône et association des fichiers `.md`
Aujourd'hui il faut lancer `cargo run` ou copier l'exécutable à la main.
- Icône de l'application et de la fenêtre.
- Installeur MSI (`cargo-wix`) ou `dx bundle`, avec association « Ouvrir avec » pour
  `.md` / `.markdown` (l'application sait déjà ouvrir un fichier passé en argument).
- Effort estimé : moyen. — **Fait** : icône Caretta (`feat/app-icon`), installeur NSIS avec associations `.md` (`feat/installer`).

## Petites améliorations repérées

### ✅ Attribut `lang` des blocs de code
Depuis la coloration par classes CSS, comrak n'écrit plus `lang="rust"` sur les
`<pre>` du HTML exporté (sans effet visible, mais l'information est perdue).
**Fait** (`fix/code-lang-attribute`) : `lang` et `data-sourcepos` sont conservés.

### ✅ Coloration syntaxique dans les exports DOCX
Les blocs de code sont en police à chasse fixe sur fond gris, mais sans couleurs
(le PDF, lui, est coloré par Typst).
**Fait** (`feat/docx-code-highlighting`) : même palette que l'export HTML.

### ❌ EPUB en plusieurs chapitres
Le livre contient un seul chapitre ; on pourrait découper aux titres de niveau 1 en
gardant les notes de bas de page avec leur chapitre.
**Non retenu** pour le moment.
