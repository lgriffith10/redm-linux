//! Localisation, en lecture seule, du Rockstar Games Launcher, de Red Dead
//! Redemption 2 et du client RedM (CitizenFX) dans un préfixe Wine.
//!
//! Ce crate encapsule la connaissance de l'organisation des fichiers de
//! logiciels propriétaires, qui évolue au gré de leurs mises à jour. Chaque
//! chemin est tracé dans `docs/references.md` avec son niveau de preuve.
//! Aucun fichier n'est jamais modifié.

use redm_core::{Finding, Resolution, Severity};
use redm_runtime::graphics::GraphicsStack;
use redm_runtime::registry::RegistryFile;
use redm_runtime::winpath::{resolve_ci, windows_to_host};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Vérifié (journal CitizenFX) : `C:\program files\rockstar games\launcher\launcher.exe`.
const LAUNCHER_EXE: &str = r"C:\Program Files\Rockstar Games\Launcher\Launcher.exe";

/// Emplacements par défaut de RDR2. Le premier est vérifié (`installLocation`
/// rapporté par le launcher dans un journal CitizenFX) ; le second est un repli.
const RDR2_DEFAULT_DIRS: &[&str] = &[
    r"C:\Program Files\Rockstar Games\Games\Red Dead Redemption 2",
    r"C:\Program Files\Rockstar Games\Red Dead Redemption 2",
];

/// Clés de registre candidates (HKLM, donc `system.reg`). Non vérifiées sur
/// une installation réelle : elles ne servent que de pistes, toujours
/// confirmées par la présence de `RDR2.exe`.
const RDR2_REGISTRY: &[(&str, &str, Rdr2Source)] = &[
    (
        r"Software\Wow6432Node\Rockstar Games\Red Dead Redemption 2",
        "InstallFolder",
        Rdr2Source::RockstarRegistry,
    ),
    (
        r"Software\Rockstar Games\Red Dead Redemption 2",
        "InstallFolder",
        Rdr2Source::RockstarRegistry,
    ),
    (
        r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Red Dead Redemption 2",
        "InstallLocation",
        Rdr2Source::UninstallRegistry,
    ),
    (
        r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Red Dead Redemption 2",
        "InstallLocation",
        Rdr2Source::UninstallRegistry,
    ),
];

/// D'où provient l'emplacement de RDR2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rdr2Source {
    RockstarRegistry,
    UninstallRegistry,
    CitizenFxIni,
    DefaultPath,
}

/// API graphique choisie dans `system.xml` (`kSettingAPI_Vulkan`, `kSettingAPI_DX12`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphicsApi {
    Vulkan,
    Dx12,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rdr2Install {
    pub path: PathBuf,
    pub source: Rdr2Source,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphics_api: Option<GraphicsApi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedmInstall {
    /// `%LOCALAPPDATA%\RedM` (vérifié : `…\AppData\Local\RedM\RedM.exe`).
    pub dir: PathBuf,
    pub exe_present: bool,
    /// `RedM.app` (vérifié : `RedM.app\data\cache\…`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_dir: Option<PathBuf>,
    /// Journaux `CitizenFX_log*.log`, du plus récent au plus ancien (5 max).
    pub latest_logs: Vec<PathBuf>,
    /// Nombre de fichiers `.dmp` sous `RedM.app/crashes`.
    pub crash_dumps: usize,
    /// Chemin du jeu configuré dans `CitizenFX.ini` (`IVPath=`), tel quel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_game_path: Option<String>,
}

/// Résultat de l'analyse d'un préfixe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameScan {
    pub prefix: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rockstar_launcher: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rdr2: Option<Rdr2Install>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redm: Option<RedmInstall>,
}

impl GameScan {
    /// Le préfixe contient au moins un des trois logiciels.
    pub fn is_relevant(&self) -> bool {
        self.rockstar_launcher.is_some() || self.rdr2.is_some() || self.redm.is_some()
    }
}

/// Répertoires utilisateur du préfixe (`drive_c/users/*`, hors `Public`).
fn user_dirs(pfx: &Path) -> Vec<PathBuf> {
    let users = resolve_ci(pfx, "drive_c/users");
    let mut v: Vec<PathBuf> = fs::read_dir(users)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() || t.is_symlink()))
        .filter(|e| !e.file_name().eq_ignore_ascii_case("public"))
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

fn existing_file(p: PathBuf) -> Option<PathBuf> {
    p.is_file().then_some(p)
}

fn rdr2_dir_if_valid(pfx: &Path, win: &str) -> Option<PathBuf> {
    let dir = windows_to_host(pfx, win)?;
    resolve_ci(&dir, "RDR2.exe").is_file().then_some(dir)
}

/// Lit `IVPath=` dans un `CitizenFX.ini`.
pub fn parse_citizenfx_ini_game_path(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim().eq_ignore_ascii_case("IVPath") && !v.trim().is_empty())
            .then(|| v.trim().to_owned())
    })
}

/// Lit l'API graphique dans le contenu de `system.xml`.
pub fn parse_graphics_api(system_xml: &str) -> Option<GraphicsApi> {
    let start = system_xml.find("kSettingAPI_")? + "kSettingAPI_".len();
    let name: String = system_xml[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    Some(match name.as_str() {
        "Vulkan" => GraphicsApi::Vulkan,
        "DX12" => GraphicsApi::Dx12,
        _ => GraphicsApi::Other(name),
    })
}

fn find_redm(pfx: &Path) -> Option<RedmInstall> {
    user_dirs(pfx).into_iter().find_map(|u| {
        let dir = resolve_ci(&u, r"AppData\Local\RedM");
        if !dir.is_dir() {
            return None;
        }
        let exe_present = resolve_ci(&dir, "RedM.exe").is_file();
        let app_dir = Some(resolve_ci(&dir, "RedM.app")).filter(|p| p.is_dir());
        let ini = [dir.clone()]
            .into_iter()
            .chain(app_dir.clone())
            .map(|d| resolve_ci(&d, "CitizenFX.ini"))
            .find_map(|p| fs::read_to_string(p).ok());
        let (latest_logs, crash_dumps) = match &app_dir {
            Some(app) => (
                find_logs(app),
                count_ext(&resolve_ci(app, "crashes"), "dmp"),
            ),
            None => (Vec::new(), 0),
        };
        Some(RedmInstall {
            dir,
            exe_present,
            app_dir,
            latest_logs,
            crash_dumps,
            configured_game_path: ini.as_deref().and_then(parse_citizenfx_ini_game_path),
        })
    })
}

/// `CitizenFX_log*.log` sous `RedM.app` (profondeur 2), plus récents d'abord.
fn find_logs(app: &Path) -> Vec<PathBuf> {
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    let mut dirs = vec![(app.to_path_buf(), 0)];
    while let Some((d, depth)) = dirs.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_lowercase();
            if path.is_dir() && depth < 2 && name != "data" {
                dirs.push((path, depth + 1));
            } else if name.starts_with("citizenfx_log") && name.ends_with(".log") {
                let t = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                found.push((t, path));
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    found.into_iter().take(5).map(|(_, p)| p).collect()
}

fn count_ext(dir: &Path, ext: &str) -> usize {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case(ext))
        })
        .count()
}

fn find_settings(pfx: &Path) -> Option<PathBuf> {
    user_dirs(pfx).into_iter().find_map(|u| {
        ["Documents", "My Documents"].iter().find_map(|docs| {
            existing_file(resolve_ci(
                &u,
                &format!(r"{docs}\Rockstar Games\Red Dead Redemption 2\Settings\system.xml"),
            ))
        })
    })
}

/// Analyse un préfixe initialisé (chemin contenant `drive_c`).
pub fn scan_prefix(pfx: &Path) -> GameScan {
    let rockstar_launcher = windows_to_host(pfx, LAUNCHER_EXE).and_then(existing_file);
    let redm = find_redm(pfx);
    let system_reg = RegistryFile::load(&pfx.join("system.reg")).unwrap_or_default();

    let from_registry = RDR2_REGISTRY.iter().find_map(|(key, name, src)| {
        let win = system_reg.get_string(key, name)?;
        rdr2_dir_if_valid(pfx, win).map(|p| (p, *src))
    });
    let from_ini = || {
        let win = redm.as_ref()?.configured_game_path.as_deref()?;
        rdr2_dir_if_valid(pfx, win).map(|p| (p, Rdr2Source::CitizenFxIni))
    };
    let from_default = || {
        RDR2_DEFAULT_DIRS
            .iter()
            .find_map(|d| rdr2_dir_if_valid(pfx, d))
            .map(|p| (p, Rdr2Source::DefaultPath))
    };

    let rdr2 = from_registry
        .or_else(from_ini)
        .or_else(from_default)
        .map(|(path, source)| {
            let settings_file = find_settings(pfx);
            let graphics_api = settings_file
                .as_deref()
                .and_then(|p| fs::read_to_string(p).ok())
                .and_then(|t| parse_graphics_api(&t));
            Rdr2Install {
                path,
                source,
                graphics_api,
                settings_file,
            }
        });

    GameScan {
        prefix: pfx.to_path_buf(),
        rockstar_launcher,
        rdr2,
        redm,
    }
}

/// Constats sur un préfixe de jeu, à la lumière de sa pile graphique.
pub fn assess_game(s: &GameScan, g: &GraphicsStack) -> Vec<Finding> {
    let mut out = Vec::new();
    let loc = s.prefix.display().to_string();
    let finding = |id: &str, sev, title: &str| {
        Finding::new(id, sev, title, Resolution::LocalConfiguration).with_location(loc.clone())
    };

    match (&s.rockstar_launcher, &s.rdr2) {
        (None, Some(_)) => out.push(
            finding("game.rdr2-without-launcher", Severity::Warning, "RDR2 présent sans Rockstar Games Launcher dans le même préfixe")
                .with_advice("Une copie Rockstar Launcher exige le launcher dans le préfixe du jeu."),
        ),
        (Some(_), None) => out.push(
            finding("game.launcher-without-rdr2", Severity::Info, "Rockstar Games Launcher présent, RDR2 non trouvé")
                .with_advice("Installer RDR2 depuis le launcher, ou signaler l'emplacement réel s'il est déjà installé."),
        ),
        _ => {}
    }

    if let Some(r) = &s.rdr2 {
        match &r.graphics_api {
            Some(GraphicsApi::Dx12) if !g.has_vkd3d_proton() => out.push(
                finding("game.rdr2-dx12-without-vkd3d-proton", Severity::Warning, "RDR2 réglé sur DirectX 12 sans VKD3D-Proton dans le préfixe")
                    .with_advice("Passer l'API sur Vulkan (`kSettingAPI_Vulkan` dans system.xml) ou lancer via Proton."),
            ),
            Some(GraphicsApi::Vulkan) => out.push(
                finding("game.rdr2-vulkan", Severity::Info, "RDR2 réglé sur l'API Vulkan : le rendu du jeu n'utilise ni DXVK ni VKD3D")
            ),
            _ => {}
        }
        if s.redm.is_none() {
            out.push(finding(
                "game.redm-not-installed",
                Severity::Info,
                "RDR2 présent, client RedM non installé dans ce préfixe",
            ));
        }
    }

    if let Some(m) = &s.redm {
        if !m.exe_present {
            out.push(finding(
                "game.redm-exe-missing",
                Severity::Warning,
                "Répertoire RedM présent mais RedM.exe absent",
            ));
        }
        if let (Some(cfg), Some(r)) = (&m.configured_game_path, &s.rdr2) {
            let resolved = windows_to_host(&s.prefix, cfg);
            if resolved.as_deref() != Some(r.path.as_path()) {
                out.push(
                    finding("game.redm-game-path-mismatch", Severity::Warning, "Le chemin du jeu configuré pour RedM ne correspond pas à l'installation de RDR2 trouvée")
                        .with_detail(format!("CitizenFX.ini : {cfg}")),
                );
            }
        }
        if let Some(latest) = m.latest_logs.first() {
            out.push(
                finding(
                    "game.redm-logs-available",
                    Severity::Info,
                    "Journaux CitizenFX disponibles",
                )
                .with_detail(latest.display().to_string())
                .with_advice("Analyser avec `redm-linux analyze-log <fichier>`."),
            );
        }
        if m.crash_dumps > 0 {
            out.push(
                finding(
                    "game.redm-crash-dumps",
                    Severity::Warning,
                    "Vidages de crash RedM présents",
                )
                .with_detail(format!("{} fichier(s) .dmp", m.crash_dumps)),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, write};

    /// Construit un préfixe factice reproduisant l'organisation vérifiée.
    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let c = tmp.path().join("drive_c");
        let launcher = c.join("Program Files/Rockstar Games/Launcher");
        let game = c.join("Program Files/Rockstar Games/Games/Red Dead Redemption 2");
        let user = c.join("users/jane");
        let redm = user.join("AppData/Local/RedM");
        let settings = user.join("Documents/Rockstar Games/Red Dead Redemption 2/Settings");
        for d in [
            &launcher,
            &game,
            &redm.join("RedM.app/logs"),
            &redm.join("RedM.app/crashes"),
            &settings,
            &c.join("users/Public"),
        ] {
            create_dir_all(d).unwrap();
        }
        write(launcher.join("Launcher.exe"), b"MZ").unwrap();
        write(game.join("RDR2.exe"), b"MZ").unwrap();
        write(redm.join("RedM.exe"), b"MZ").unwrap();
        write(
            redm.join("RedM.app/logs/CitizenFX_log_2026-01-01T000000.log"),
            "x",
        )
        .unwrap();
        write(redm.join("RedM.app/crashes/a.dmp"), "x").unwrap();
        write(
            settings.join("system.xml"),
            "<advancedGraphics><API>kSettingAPI_Vulkan</API></advancedGraphics>",
        )
        .unwrap();
        write(
            tmp.path().join("system.reg"),
            "WINE REGISTRY Version 2\n#arch=win64\n",
        )
        .unwrap();
        tmp
    }

    #[test]
    fn finds_launcher_game_and_redm_by_default_paths() {
        let tmp = fixture();
        let s = scan_prefix(tmp.path());
        assert!(s.is_relevant());
        assert!(
            s.rockstar_launcher
                .unwrap()
                .ends_with("Launcher/Launcher.exe")
        );
        let r = s.rdr2.unwrap();
        assert_eq!(r.source, Rdr2Source::DefaultPath);
        assert!(r.path.ends_with("Games/Red Dead Redemption 2"));
        assert_eq!(r.graphics_api, Some(GraphicsApi::Vulkan));
        let m = s.redm.unwrap();
        assert!(m.exe_present);
        assert_eq!(m.latest_logs.len(), 1);
        assert_eq!(m.crash_dumps, 1);
    }

    #[test]
    fn registry_location_wins_and_is_case_insensitive() {
        let tmp = fixture();
        let other = tmp.path().join("drive_c/Jeux/RDR2");
        create_dir_all(&other).unwrap();
        write(other.join("rdr2.EXE"), b"MZ").unwrap();
        write(
            tmp.path().join("system.reg"),
            "WINE REGISTRY Version 2\n#arch=win64\n\n\
             [Software\\\\Wow6432Node\\\\Rockstar Games\\\\Red Dead Redemption 2] 1\n\
             \"InstallFolder\"=\"c:\\\\jeux\\\\rdr2\"\n",
        )
        .unwrap();
        let r = scan_prefix(tmp.path()).rdr2.unwrap();
        assert_eq!(r.source, Rdr2Source::RockstarRegistry);
        assert_eq!(r.path, other);
    }

    #[test]
    fn registry_entry_without_exe_is_ignored() {
        let tmp = fixture();
        write(
            tmp.path().join("system.reg"),
            "[Software\\\\Wow6432Node\\\\Rockstar Games\\\\Red Dead Redemption 2] 1\n\
             \"InstallFolder\"=\"C:\\\\Nowhere\"\n",
        )
        .unwrap();
        assert_eq!(
            scan_prefix(tmp.path()).rdr2.unwrap().source,
            Rdr2Source::DefaultPath
        );
    }

    #[test]
    fn empty_prefix_is_not_relevant() {
        let tmp = tempfile::tempdir().unwrap();
        create_dir_all(tmp.path().join("drive_c/users/jane")).unwrap();
        assert!(!scan_prefix(tmp.path()).is_relevant());
    }

    #[test]
    fn parsers() {
        assert_eq!(
            parse_graphics_api("<API>kSettingAPI_DX12</API>"),
            Some(GraphicsApi::Dx12)
        );
        assert_eq!(parse_graphics_api("<x/>"), None);
        assert_eq!(
            parse_citizenfx_ini_game_path(
                "[Game]\nIVPath=C:\\Games\\RDR2\nUpdateChannel=production\n"
            )
            .as_deref(),
            Some("C:\\Games\\RDR2")
        );
        assert_eq!(parse_citizenfx_ini_game_path("[Game]\nIVPath=\n"), None);
    }

    #[test]
    fn assessment() {
        let tmp = fixture();
        let s = scan_prefix(tmp.path());
        let g = GraphicsStack {
            dlls: vec![],
            dll_overrides: Default::default(),
            dxvk_version: None,
            vkd3d_proton_version: None,
        };
        let ids: Vec<_> = assess_game(&s, &g).into_iter().map(|f| f.rule_id).collect();
        assert_eq!(
            ids,
            [
                "game.rdr2-vulkan",
                "game.redm-logs-available",
                "game.redm-crash-dumps"
            ]
        );

        let mut s2 = s.clone();
        s2.rockstar_launcher = None;
        s2.redm = None;
        if let Some(r) = s2.rdr2.as_mut() {
            r.graphics_api = Some(GraphicsApi::Dx12);
        }
        let ids: Vec<_> = assess_game(&s2, &g)
            .into_iter()
            .map(|f| f.rule_id)
            .collect();
        assert_eq!(
            ids,
            [
                "game.rdr2-without-launcher",
                "game.rdr2-dx12-without-vkd3d-proton",
                "game.redm-not-installed"
            ]
        );
    }
}
