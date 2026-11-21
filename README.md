# redm-linux

Plateforme d'ingénierie de compatibilité, écrite en Rust, pour faire fonctionner
**RedM (Cfx.re)** sous **Linux** via Wine/Proton, au moyen de correctifs
légitimes, maintenables et upstreamables.

Ce n'est pas un lanceur. L'outil diagnostique l'environnement, inventorie les
runtimes, analyse les journaux, et produit des **preuves reproductibles**
(JSON versionné, attribué à un hôte).

Le brief d'origine du projet est conservé dans [`docs/brief.md`](docs/brief.md).

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
./target/release/redm-linux runtimes             # Proton + versions DXVK/VKD3D-Proton
./target/release/redm-linux scan                 # préfixes contenant Rockstar Launcher / RDR2 / RedM
./target/release/redm-linux prefix ~/.wine       # analyse complète d'un préfixe
WINEDEBUG=+loaddll wine app.exe 2>&1 | ./target/release/redm-linux analyze-log -
```

La sortie `--json` est **anonymisée par défaut** (e-mail du compte Rockstar,
noms d'utilisateur Windows/Linux) pour pouvoir être publiée telle quelle.
`--no-redact` désactive ce masquage, pour le débogage local uniquement.

Codes de sortie : `0` = aucun constat ≥ error, `1` = échec de l'outil,
`2` = au moins une erreur, `3` = au moins un bloquant.

## Organisation

| Crate          | Rôle                                                              |
|----------------|-------------------------------------------------------------------|
| `redm-core`    | Vocabulaire commun : `HostKind`, `Finding`, `Resolution`, `EvidenceRecord`, anonymisation. Aucune E/S. |
| `redm-env`     | Détection de l'hôte (WSL2/natif, CPU, RAM, Vulkan, Wine) et règles associées. |
| `redm-runtime` | Proton (dont DXVK/VKD3D-Proton embarqués), préfixes, registre Wine, pile graphique. |
| `redm-game`    | Rockstar Games Launcher, RDR2 (emplacement, API graphique), RedM (journaux, crashs). Lecture seule. |
| `redm-logs`    | Analyse des journaux Wine/Vulkan/CitizenFX à base de règles.      |
| `redm-cli`     | Binaire `redm-linux`.                                             |

L'origine et le niveau de preuve de chaque chemin ou marqueur codé en dur
sont tracés dans [`docs/references.md`](docs/references.md).

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
