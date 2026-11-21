# Politique du projet

## Anti-triche et sécurité

Le projet **ne** fait **jamais** :

- contourner, désactiver, usurper ou neutraliser Adhesive (l'anti-triche de Cfx.re) ;
- modifier, patcher ou injecter du code dans des binaires propriétaires
  (CitizenFX, RDR2, Rockstar Launcher) pour lever une restriction de sécurité ;
- falsifier l'identité de la plateforme (faire passer Wine pour Windows) dans le
  but de tromper un mécanisme de sécurité ;
- affaiblir un mécanisme de sécurité de l'hôte (désactiver SELinux/AppArmor,
  ptrace global, etc.).

Quand un problème relève de ces mécanismes, il est classé
`Resolution::RequiresCfxAuthorization` et documenté pour Cfx.re (versions
exactes, journaux, comportement observé). Le travail s'arrête là tant que
Cfx.re n'a pas fourni une autorisation ou un correctif. La règle
`anticheat.adhesive` de `redm-logs` applique cette classification
automatiquement.

## Classification des problèmes

Chaque `Finding` porte une `Resolution` :

| Valeur                       | Sens                                                     |
|------------------------------|----------------------------------------------------------|
| `local-configuration`        | Corrigeable par l'utilisateur (paquet, config, matériel). |
| `upstream { project }`       | Défaut technique : patch à proposer à Wine, Proton, DXVK, VKD3D-Proton, Mesa ou CitizenFX. |
| `requires-cfx-authorization` | Ne peut être traité que par Cfx.re.                      |
| `unknown`                    | Preuves insuffisantes.                                    |

## Preuves

- Toute affirmation de compatibilité renvoie à un `EvidenceRecord` JSON
  (`redm-linux --json …`) stocké avec le rapport.
- Chaque enregistrement indique `host` (`native-linux`, `wsl2`…). Seul
  `native-linux` permet une conclusion de compatibilité finale. Sous WSL2, le GPU
  passe par `/dev/dxg` et Vulkan éventuellement par Dozen (D3D12), ce qui n'est
  pas représentatif.
- Les journaux de test versionnés dans le dépôt sont soit **synthétiques**
  (et nommés `synthetic-*`), soit des captures réelles accompagnées de leur
  `EvidenceRecord` d'environnement.

## Patches

`patches/<projet>/` contient uniquement des correctifs destinés à être proposés
upstream. Chaque patch doit :

1. cibler un projet libre (Wine, Proton, DXVK, VKD3D-Proton, Mesa, ou CitizenFX
   dans le cadre de sa licence et de ses règles de contribution) ;
2. être accompagné d'un test (test de conformance Wine pour Wine) ;
3. référencer la preuve qui le motive ;
4. respecter le format attendu par le projet amont (`git format-patch`, signature).
