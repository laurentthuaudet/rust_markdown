# Embedded fonts

Caretta typefaces, embedded so the interface looks the same offline and never
contacts a font service (see `src/fonts.rs`).

| Font | Files | License |
|------|-------|---------|
| Fraunces (variable, weight 400–600, optical size) | `Fraunces-normal-latin*.woff2` | SIL OFL 1.1 — `OFL-fraunces.txt` |
| Instrument Sans (variable, weight 400–600) | `InstrumentSans-normal-latin*.woff2` | SIL OFL 1.1 — `OFL-instrumentsans.txt` |
| JetBrains Mono (400) | `JetBrainsMono-normal-latin*.woff2` | SIL OFL 1.1 — `OFL-jetbrainsmono.txt` |

The WOFF2 files are the Latin and Latin Extended subsets served by Google
Fonts; the licenses come from the google/fonts repository.
