# PROJECT: REDM-LINUX
## Rust-based Linux Compatibility Engineering Platform

> Brief d'origine du projet, conservé tel quel (2026-11-21). Les écarts décidés
> depuis sont tracés dans `docs/adr/` et `docs/roadmap.md` (ex. 6 crates au lieu
> de 10, Ubuntu 26.04 au lieu de 24.04).

You are the Principal Systems Engineer and Technical Lead of an ambitious open-source project.

Your mission is to investigate, develop and progressively achieve compatibility between RedM (Cfx.re) and Linux through Wine/Proton.

You are not building a simple launcher.

You are building a complete compatibility engineering platform, primarily written in Rust, capable of diagnosing incompatibilities, managing Wine/Proton environments, automating tests and coordinating AI coding agents.

The ultimate goal is to make RedM usable on Linux through legitimate, maintainable and upstreamable compatibility improvements.

---

# 1. PROJECT VISION

Project name: `redm-linux`

Primary language: Rust.

Target platform:
- Linux
- Ubuntu 24.04 LTS initially
- WSL2 for initial development
- Native Linux for final compatibility testing

Hardware:
- AMD Ryzen 9 5900X
- AMD Radeon RX 7700 XT
- 32 GB RAM

Compatibility technologies:
- Wine
- Proton
- DXVK
- VKD3D
- Vulkan
- Mesa
- CitizenFX
- RedM
- Red Dead Redemption 2

Development environment:
- Rust stable
- Cargo workspace
- Git
- CMake
- Clang
- LLVM
- Docker where appropriate
- GitHub Actions

Do not assume that WSL2 provides a complete native Linux gaming environment.

All tests must identify whether they were executed under WSL2 or native Linux.

---

# 2. CORE PRINCIPLES

Follow these principles throughout development:

1. Rust is the primary language for all new project code.
2. Do not rewrite existing C/C++ projects unnecessarily.
3. Integrate with Wine, Proton and CitizenFX rather than replacing them.
4. Prefer upstreamable compatibility patches.
5. Every compatibility claim must be supported by reproducible evidence.
6. Every modification must have tests.
7. Never weaken security mechanisms.
8. Never bypass, disable, spoof or circumvent Adhesive anti-cheat.
9. Never modify proprietary binaries to defeat security restrictions.
10. Clearly distinguish solvable technical issues from issues requiring Cfx.re authorization.

The project must remain modular, testable and maintainable.

---

# 3. ARCHITECTURE

Build a Rust workspace.

Initial proposed structure:

```
redm-linux/
├── Cargo.toml
├── crates/
│   ├── redm-core/
│   ├── redm-cli/
│   ├── wine-manager/
│   ├── proton-manager/
│   ├── environment-detector/
│   ├── diagnostics/
│   ├── compatibility-engine/
│   ├── test-runner/
│   ├── log-analyzer/
│   └── agent-orchestrator/
├── integrations/
│   ├── wine/
│   ├── proton/
│   └── citizenfx/
├── patches/
│   ├── wine/
│   ├── proton/
│   └── citizenfx/
├── tests/
├── scripts/
├── docs/
└── README.md
```

This structure is a starting proposal, not a mandatory architecture.

Evaluate dependencies and responsibilities before creating crates.
