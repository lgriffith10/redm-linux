# redm-linux

Plateforme d'ingénierie de compatibilité, écrite en Rust, pour faire fonctionner
**RedM (Cfx.re)** sous **Linux** via Wine/Proton, au moyen de correctifs
légitimes, maintenables et upstreamables.

Ce n'est pas un lanceur. L'outil diagnostique l'environnement, inventorie les
runtimes, analyse les journaux, et produit des **preuves reproductibles**
(JSON versionné, attribué à un hôte).

> **Statut : phase 0 (fondations).** Aucune compatibilité RedM n'est revendiquée
> à ce stade. Voir [`docs/roadmap.md`](docs/roadmap.md).

## Règles non négociables

- Ne jamais contourner, désactiver, usurper ni neutraliser l'anti-triche
  **Adhesive**, et ne jamais modifier de binaire propriétaire pour lever une
  restriction de sécurité. Tout constat qui en dépend est classé
  `requires-cfx-authorization` et remonté à Cfx.re. Voir [`docs/policy.md`](docs/policy.md).
- Toute affirmation de compatibilité repose sur une preuve reproductible, qui
  indique si elle a été obtenue sous **WSL2** ou sous **Linux natif**.

## Démarrage

```sh
cargo build --release
./target/release/redm-linux doctor               # diagnostic complet
./target/release/redm-linux --json env           # preuve JSON de l'hôte
./target/release/redm-linux runtimes             # installations Proton
./target/release/redm-linux prefix ~/.wine       # inspection d'un préfixe
WINEDEBUG=+loaddll wine app.exe 2>&1 | ./target/release/redm-linux analyze-log -
```

Codes de sortie : `0` = aucun constat ≥ error, `1` = échec de l'outil,
`2` = au moins une erreur, `3` = au moins un bloquant.

## Organisation

| Crate          | Rôle                                                              |
|----------------|-------------------------------------------------------------------|
| `redm-core`    | Vocabulaire commun : `HostKind`, `Finding`, `Resolution`, `EvidenceRecord`. Aucune E/S. |
| `redm-env`     | Détection de l'hôte (WSL2/natif, CPU, RAM, Vulkan, Wine) et règles associées. |
| `redm-runtime` | Découverte de Proton, inspection des préfixes Wine/Proton.        |
| `redm-logs`    | Analyse des journaux Wine/Vulkan à base de règles.                |
| `redm-cli`     | Binaire `redm-linux`.                                             |

Le découpage retenu, et l'écart avec la structure à 10 crates proposée
initialement, sont justifiés dans [`docs/adr/0001-workspace-structure.md`](docs/adr/0001-workspace-structure.md).

`integrations/` et `patches/` accueilleront le code et les correctifs destinés à
Wine, Proton et CitizenFX (voir [`docs/policy.md`](docs/policy.md#patches)).

## Développement

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`unsafe` est interdit dans tout le workspace (`forbid(unsafe_code)`).
