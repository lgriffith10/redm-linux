# Feuille de route

## Phase 0 — Fondations (en cours)

- [x] Workspace Rust, lints, CI (fmt, clippy, tests)
- [x] Modèle de preuve (`EvidenceRecord`) attribué à un hôte
- [x] Détection WSL1/WSL2/natif, CPU, RAM, Vulkan, Wine
- [x] Découverte de Proton, inspection des préfixes
- [x] Analyse des journaux à base de règles (Wine/Vulkan génériques, Adhesive)
- [x] Détection des versions de DXVK et VKD3D-Proton dans un préfixe ou une installation Proton (vérifiée sur DXVK 3.1.1 / VKD3D-Proton 3.1)
- [x] Localisation du Rockstar Launcher, de RDR2 et du client RedM (lecture seule)
- [x] Règles CitizenFX issues d'un journal réel ; anonymisation des preuves JSON

## Phase 1 — Banc natif

Prérequis : un hôte **Linux natif** : Ubuntu 26.04 LTS en dual-boot (Ryzen 9 5900X / RX 7700 XT, Mesa RADV).
La spec initiale visait 24.04 ; 26.04 a été retenue car c'est aussi la distribution de l'hôte WSL2 de
développement (Mesa 26.0.3), ce qui aligne les deux environnements. La CI reste sur `ubuntu-24.04` pour
vérifier que l'outil compile aussi sur l'ancienne LTS.

- [ ] Collecte des premières preuves `native-linux` : `doctor`, RDR2 seul sous Proton (Rockstar Launcher dans un préfixe Proton/Lutris)
- [ ] Confirmer les « pistes » de `docs/references.md` (journaux RedM, registre du launcher, préfixe Proton)
- [ ] Capture structurée des journaux d'un lancement RedM (`WINEDEBUG` ciblé, journaux CitizenFX)
- [ ] Signatures `redm-logs` issues de journaux réels
- [ ] Création de `test-runner` (critères : voir ADR 0001)

## Phase 2 — Triage

- [ ] Classement de chaque échec : configuration locale, amont (Wine/Proton/DXVK/…) ou `requires-cfx-authorization`
- [ ] Rapports à Cfx.re pour les points qui relèvent de son autorisation
- [ ] Premiers patches upstreamables, avec tests, dans `patches/`

## Phase 3 — Matrice et agents

- [ ] `compatibility-engine` : matrice de versions × résultats, alimentée par les preuves
- [ ] `agent-orchestrator` : coordination d'agents IA sur la CLI JSON
