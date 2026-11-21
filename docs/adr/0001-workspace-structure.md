# ADR 0001 — Structure du workspace

- Statut : accepté
- Date : 2026-11-21

## Contexte

La spécification initiale proposait 10 crates : `redm-core`, `redm-cli`,
`wine-manager`, `proton-manager`, `environment-detector`, `diagnostics`,
`compatibility-engine`, `test-runner`, `log-analyzer`, `agent-orchestrator`.
Elle demandait explicitement d'évaluer dépendances et responsabilités avant
de créer les crates.

Un crate a un coût : un manifeste, une API publique à stabiliser, un graphe de
dépendances. Il ne se justifie que par une responsabilité distincte, et
idéalement une frontière de dépendances distincte.

## Décision

Créer 5 crates en phase 0 :

| Retenu         | Remplace                                   | Justification |
|----------------|--------------------------------------------|---------------|
| `redm-core`    | `redm-core`                                | Vocabulaire partagé, sans E/S. Seule dépendance commune à tous. |
| `redm-env`     | `environment-detector` + règles d'env      | La collecte et son évaluation évoluent ensemble. |
| `redm-runtime` | `wine-manager` + `proton-manager`          | Proton est une distribution de Wine ; les deux partagent la notion de préfixe. Les séparer dupliquerait la gestion des préfixes. |
| `redm-logs`    | `log-analyzer`                             | Seul crate à dépendre de `regex`. |
| `redm-cli`     | `redm-cli`                                 | Composition ; seul à dépendre de `clap`/`anyhow`. |

Les règles de diagnostic vivent à côté des données qu'elles évaluent
(`redm_env::assess`, `redm_runtime::assess_*`, règles de `redm-logs`) et
produisent toutes des `redm_core::Finding`. Il n'y a donc pas de crate
`diagnostics` séparé.

### Reportés, avec le critère de création

| Crate                  | Créé quand…                                                                 |
|------------------------|-----------------------------------------------------------------------------|
| `compatibility-engine` | Une matrice de compatibilité (versions Wine/Proton/DXVK × résultats) est alimentée par des preuves réelles et doit être interrogée. |
| `test-runner`          | Un premier scénario de lancement reproductible existe sur un hôte **Linux natif** (exécution, capture des journaux, comparaison). |
| `agent-orchestrator`   | Le format `EvidenceRecord` est stabilisé (schéma v1 gelé) et au moins deux flux de travail d'agents sont décrits. Il consommera la CLI JSON plutôt que d'en dupliquer la logique. |

## Conséquences

- Moins de surface d'API à maintenir pendant que le domaine est encore mal connu.
- Diviser un crate plus tard est facile (déplacer un module). Fusionner des
  crates dont les API ont divergé est coûteux.
- Toute la collecte système passe par le trait `redm_env::SystemSource`, ce qui
  rend les règles testables sans l'hôte réel.

## Addendum (2026-11-21) — crate `redm-game`

Un sixième crate, `redm-game`, a été créé pour localiser le Rockstar Games
Launcher, RDR2 et RedM dans un préfixe. Ce n'est pas un module de
`redm-runtime` pour deux raisons :

- **Responsabilité distincte** : `redm-runtime` décrit Wine/Proton (logiciels
  libres, formats stables) ; `redm-game` encode l'organisation de fichiers de
  logiciels propriétaires, qui change avec leurs mises à jour et dont chaque
  élément porte un niveau de preuve (`docs/references.md`).
- **Sens des dépendances** : `redm-game` dépend de `redm-runtime` (registre,
  chemins Windows, pile graphique), jamais l'inverse.

`redm-runtime` gagne les modules `registry` (lecture des `.reg` de Wine),
`winpath` (chemins Windows → hôte, insensibles à la casse), `dll`
(identification Wine/DXVK/VKD3D-Proton) et `graphics`.
