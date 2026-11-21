# Références et niveau de preuve

Chaque chemin, clé ou marqueur codé en dur dans le projet est listé ici avec
sa source. Niveaux :

- **vérifié** : observé directement sur un binaire ou un fichier réel ;
- **documenté** : source officielle (éditeur) sans observation directe ;
- **piste** : source tierce ou déduction. Toujours confirmé à l'exécution
  (ex. présence de `RDR2.exe`), jamais pris pour acquis.

Dernière revue : 2026-11-21.

## Traducteurs graphiques (`redm-runtime/src/dll.rs`)

| Marqueur | Niveau | Source |
|----------|--------|--------|
| `Wine builtin DLL` à l'offset 0x40 | vérifié | `libwine_10.0~repack-12ubuntu1_amd64.deb` (Ubuntu 26.04), `x86_64-windows/{d3d11,dxgi,d3d12,d3d12core}.dll` |
| `Wine placeholder DLL` à l'offset 0x40 | piste | chaîne présente dans `setupapi.dll` de Wine 10.0 (qui écrit ces DLL fictives) ; pas encore observée dans un préfixe |
| `DXVK: \0v3.1.1\0` | vérifié | [DXVK v3.1.1](https://github.com/doitsujin/dxvk/releases/tag/v3.1.1), `x64`/`x32` `d3d11`, `dxgi`, `d3d9` |
| DXVK `d3d8.dll`, `d3d10core.dll` sans version ; `d3d10core.dll` sans aucun marqueur | vérifié | idem |
| DXVK contient « vkd3d-proton » (interop D3D11On12) | vérifié | idem : d'où la détection de DXVK **avant** VKD3D-Proton |
| `vkd3d\0` + `3.1.0\0` dans `d3d12core.dll` ; « vkd3d-proton » dans `d3d12.dll`/`d3d12core.dll` ; aucune occurrence de « DXVK » | vérifié | [VKD3D-Proton v3.1](https://github.com/HansKristian-Work/vkd3d-proton/releases/tag/v3.1), `x64`/`x86` |

Le test `dll::tests::real_release_binaries` (ignoré par défaut) rejoue ces
vérifications sur les archives officielles :

```sh
REDM_DXVK_DIR=…/dxvk-3.1.1 REDM_VKD3D_DIR=…/vkd3d-proton-3.1 \
REDM_WINE_DLL_DIR=…/wine/x86_64-windows \
cargo test -p redm-runtime -- --include-ignored
```

## Rockstar / RDR2 / RedM (`redm-game`)

Sources : un journal CitizenFX RedM public du
[forum Cfx.re](https://forum.cfx.re/uploads/short-url/lSyEkowvbidX0Cl2CjqaSIzy5rI.log)
(avril 2024, build 1491) ; un extrait anonymisé est versionné dans
`crates/redm-logs/tests/fixtures/citizenfx-redm-excerpt.log`.

| Élément | Niveau | Source |
|---------|--------|--------|
| `C:\Program Files\Rockstar Games\Launcher\Launcher.exe` | vérifié | journal CitizenFX (`ros:launcher … "C:\program files\rockstar games\launcher\launcher.exe"`) |
| RDR2 dans `C:\Program Files\Rockstar Games\Games\Red Dead Redemption 2` | vérifié | journal CitizenFX (`installLocation` rapporté par le launcher) |
| `RDR2.exe` | documenté | [support Rockstar](https://support.rockstargames.com/articles/6JGT3mscb7rKyB9qFQvR9h/error-unable-to-launch-rdr2-exe-due-to-lack-of-privileges-on-the-game-library-directory) |
| `%LOCALAPPDATA%\RedM\RedM.exe`, `RedM.app\data\cache\…` | vérifié | journal CitizenFX (`hello from "C:\Users\…\AppData\Local\RedM\RedM.exe"`) |
| `Documents\Rockstar Games\Red Dead Redemption 2\Settings\system.xml`, `kSettingAPI_Vulkan` / `kSettingAPI_DX12` | documenté | [support Rockstar, ERR_GFX_INIT](https://support.rockstargames.com/articles/2MQafAtBWiOUJrPDLx5I2U/err-gfx-init-error-when-launching-red-dead-redemption-2-on-pc) |
| Journaux `CitizenFX_log*.log` sous `RedM.app` | piste | nom observé dans les fils du forum ; emplacement exact non confirmé (recherche jusqu'à 2 niveaux sous `RedM.app`) |
| `RedM.app\crashes\*.dmp` | piste | analogie avec FiveM |
| `CitizenFX.ini` / `IVPath=` | piste | analogie avec FiveM |
| `HKLM\Software\Wow6432Node\Rockstar Games\Red Dead Redemption 2` | piste | clé laissée par la désinstallation ([advanceduninstaller](https://www.advanceduninstaller.com/Red-Dead-Redemption-2-4f16995fc51943d500539d72bd8247de-application.htm)) ; valeur `InstallFolder` **non confirmée** |
| `…\CurrentVersion\Uninstall\Red Dead Redemption 2` / `InstallLocation` | piste | [Nexus Mods](https://www.nexusmods.com/reddeadredemption2/mods/86?tab=posts) |

## Journaux CitizenFX (`redm-logs`)

| Règle / fait | Niveau | Observation |
|--------------|--------|-------------|
| `pre-gameload component adhesive` est **normal** | vérifié | présent dans un démarrage Windows ordinaire : ne doit pas être traité comme un échec |
| `Process crash captured. Crash dialog content:` puis `<module>!<symbole>` | vérifié | ligne `[bNNNN_DumpServ]` suivante |
| `--- BEGIN LOGGING AT`, préfixe `[b1491_…]`, `"currentVersion":"1.0.1491.18"` | vérifié | même journal |
| Le journal contient l'e-mail du compte Rockstar et le nom d'utilisateur Windows | vérifié | `External ROS says it's signed in: <email>` : d'où l'anonymisation obligatoire |

## À confirmer sur le banc natif

- Emplacement réel des journaux RedM et des vidages de crash.
- Clés de registre écrites par le Rockstar Launcher sous Wine.
- Contenu de `system32` d'un préfixe Proton (liens symboliques vers `files/lib/wine/…`).
- Organisation `files/lib/wine/dxvk` et `files/lib/wine/vkd3d-proton` des versions Proton installées.
