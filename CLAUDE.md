# redm-linux — instructions pour les agents

Brief d'origine du projet (mission, principes, contraintes) :

@docs/brief.md

À lire aussi avant toute modification : `docs/policy.md` (anti-triche,
preuves, données personnelles), `docs/references.md` (niveau de preuve des
chemins et marqueurs), `docs/adr/`, `docs/roadmap.md`.

Vérifications avant commit :

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
