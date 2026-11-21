//! Pile graphique d'un préfixe (DLL de `system32` + surcharges) et versions
//! de DXVK / VKD3D-Proton embarquées dans une installation Proton.

use crate::dll::{DllInfo, DllKind, inspect_dll};
use crate::registry::RegistryFile;
use redm_core::{Finding, Resolution, Severity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// DLL graphiques inspectées dans `system32` (64 bits dans un préfixe win64).
pub const GRAPHICS_DLLS: &[&str] = &[
    "dxgi.dll",
    "d3d11.dll",
    "d3d10core.dll",
    "d3d9.dll",
    "d3d12.dll",
    "d3d12core.dll",
];

const DLL_OVERRIDES_KEY: &str = r"Software\Wine\DllOverrides";

/// Pile graphique d'un préfixe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphicsStack {
    pub dlls: Vec<DllInfo>,
    /// Surcharges `user.reg` (`d3d11` → `native,builtin`). Proton passe en plus
    /// par `WINEDLLOVERRIDES` au lancement, invisible ici.
    pub dll_overrides: BTreeMap<String, String>,
    pub dxvk_version: Option<String>,
    pub vkd3d_proton_version: Option<String>,
}

impl GraphicsStack {
    pub fn kind_of(&self, dll: &str) -> Option<DllKind> {
        self.dlls.iter().find(|d| d.name == dll).map(|d| d.kind)
    }
    pub fn has_dxvk(&self) -> bool {
        self.dlls.iter().any(|d| d.kind == DllKind::Dxvk)
    }
    pub fn has_vkd3d_proton(&self) -> bool {
        self.dlls.iter().any(|d| d.kind == DllKind::Vkd3dProton)
    }
}

fn first_version(dlls: &[DllInfo], kind: DllKind) -> Option<String> {
    dlls.iter()
        .filter(|d| d.kind == kind)
        .find_map(|d| d.version.clone())
}

/// Inspecte la pile graphique d'un préfixe initialisé (chemin `pfx`).
pub fn inspect_prefix_graphics(pfx: &Path) -> GraphicsStack {
    let sys32 = crate::winpath::resolve_ci(pfx, "drive_c/windows/system32");
    let dlls: Vec<DllInfo> = GRAPHICS_DLLS
        .iter()
        .filter_map(|n| inspect_dll(&sys32.join(n)))
        .collect();
    let dll_overrides = RegistryFile::load(&pfx.join("user.reg"))
        .map(|r| {
            r.strings_in(DLL_OVERRIDES_KEY)
                .into_iter()
                .map(|(k, v)| (k.trim_start_matches('*').to_owned(), v.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    GraphicsStack {
        dxvk_version: first_version(&dlls, DllKind::Dxvk),
        vkd3d_proton_version: first_version(&dlls, DllKind::Vkd3dProton),
        dlls,
        dll_overrides,
    }
}

/// Constats sur la pile graphique d'un préfixe.
pub fn assess_graphics(g: &GraphicsStack) -> Vec<Finding> {
    let mut out = Vec::new();

    let placeholders: Vec<_> = g
        .dlls
        .iter()
        .filter(|d| d.kind == DllKind::WinePlaceholder)
        .map(|d| d.name.as_str())
        .collect();
    if !placeholders.is_empty() {
        out.push(
            Finding::new(
                "graphics.placeholder-dll",
                Severity::Warning,
                "DLL graphiques fictives (« Wine placeholder ») dans system32",
                Resolution::LocalConfiguration,
            )
            .with_detail(placeholders.join(", "))
            .with_advice("Le préfixe a été créé par un Wine sans ces DLL ; le recréer ou installer DXVK/VKD3D-Proton."),
        );
    }

    // Surcharge `native` seule alors que la DLL présente est celle de Wine :
    // le chargement échouera.
    for (dll, mode) in &g.dll_overrides {
        let file = format!("{dll}.dll");
        let native_only =
            mode.split(',').next().map(str::trim) == Some("native") && !mode.contains("builtin");
        if native_only && g.kind_of(&file) == Some(DllKind::WineBuiltin) {
            out.push(
                Finding::new(
                    "graphics.native-override-without-native-dll",
                    Severity::Error,
                    "Surcharge « native » sans DLL native correspondante",
                    Resolution::LocalConfiguration,
                )
                .with_detail(format!(
                    "{dll} = {mode}, mais {file} est la DLL intégrée de Wine"
                )),
            );
        }
    }

    if !g.has_vkd3d_proton() && g.kind_of("d3d12.dll").is_some() {
        out.push(
            Finding::new(
                "graphics.no-vkd3d-proton",
                Severity::Warning,
                "VKD3D-Proton absent du préfixe : Direct3D 12 passera par le vkd3d de Wine",
                Resolution::LocalConfiguration,
            )
            .with_advice(
                "Sans importance si RDR2 est réglé sur l'API Vulkan. Sinon, utiliser Proton \
                 ou installer VKD3D-Proton dans le préfixe.",
            ),
        );
    }

    if !g.has_dxvk() && g.kind_of("d3d11.dll").is_some() {
        out.push(
            Finding::new(
                "graphics.no-dxvk",
                Severity::Info,
                "DXVK absent du préfixe : Direct3D 9/10/11 passera par WineD3D",
                Resolution::LocalConfiguration,
            )
            .with_advice(
                "Proton installe DXVK au lancement ; avec Wine seul, l'installer dans le préfixe. \
                 CitizenFX charge d3d11.dll (observé dans un journal de crash RedM).",
            ),
        );
    }

    out
}

/// Versions de DXVK / VKD3D-Proton embarquées dans une installation Proton.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundledTranslators {
    pub dxvk_version: Option<String>,
    pub vkd3d_proton_version: Option<String>,
}

/// Parcourt `files/` (Proton ≥ 5) ou `dist/` (ancien) à la recherche des DLL
/// de DXVK et VKD3D-Proton, en privilégiant les variantes 64 bits.
pub fn inspect_proton_bundle(proton_dir: &Path) -> BundledTranslators {
    let mut candidates = Vec::new();
    for sub in ["files", "dist"] {
        collect_named(
            &proton_dir.join(sub),
            &["d3d11.dll", "dxgi.dll", "d3d12core.dll"],
            7,
            &mut candidates,
        );
    }
    // 64 bits d'abord.
    candidates.sort_by_key(|p| {
        let s = p.to_string_lossy();
        !(s.contains("x86_64") || s.contains("x64") || s.contains("lib64"))
    });
    let dlls: Vec<DllInfo> = candidates.iter().filter_map(|p| inspect_dll(p)).collect();
    BundledTranslators {
        dxvk_version: first_version(&dlls, DllKind::Dxvk),
        vkd3d_proton_version: first_version(&dlls, DllKind::Vkd3dProton),
    }
}

fn collect_named(dir: &Path, names: &[&str], depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let path = e.path();
        if ft.is_dir() {
            collect_named(&path, names, depth - 1, out);
        } else if names.iter().any(|n| e.file_name().eq_ignore_ascii_case(n)) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{create_dir_all, write};

    fn pe(marker_0x40: &[u8], body: &[u8]) -> Vec<u8> {
        let mut v = b"MZ".to_vec();
        v.resize(0x40, 0);
        v.extend_from_slice(marker_0x40);
        v.resize(0x100, 0);
        v.extend_from_slice(body);
        v
    }

    fn prefix_with(dlls: &[(&str, Vec<u8>)], user_reg: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let sys = tmp.path().join("drive_c/windows/system32");
        create_dir_all(&sys).unwrap();
        for (n, b) in dlls {
            write(sys.join(n), b).unwrap();
        }
        write(tmp.path().join("user.reg"), user_reg).unwrap();
        tmp
    }

    #[test]
    fn plain_wine_prefix() {
        let builtin = pe(b"Wine builtin DLL", b"");
        let tmp = prefix_with(
            &[
                ("d3d11.dll", builtin.clone()),
                ("dxgi.dll", builtin.clone()),
                ("d3d12.dll", builtin),
            ],
            "WINE REGISTRY Version 2\n",
        );
        let g = inspect_prefix_graphics(tmp.path());
        assert_eq!(g.dlls.len(), 3);
        assert!(!g.has_dxvk());
        let ids: Vec<_> = assess_graphics(&g).into_iter().map(|f| f.rule_id).collect();
        assert_eq!(ids, ["graphics.no-vkd3d-proton", "graphics.no-dxvk"]);
    }

    #[test]
    fn dxvk_prefix_with_overrides() {
        let dxvk = pe(b"", b"\0DXVK: \0v3.1.1\0");
        let vkd3d = pe(b"", b"\0vkd3d\x003.1.0\0vkd3d-proton");
        let tmp = prefix_with(
            &[
                ("d3d11.dll", dxvk.clone()),
                ("dxgi.dll", dxvk),
                ("d3d12core.dll", vkd3d),
                ("d3d9.dll", pe(b"Wine builtin DLL", b"")),
            ],
            "WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\DllOverrides] 1\n\
             \"d3d11\"=\"native\"\n\"*d3d9\"=\"native\"\n",
        );
        let g = inspect_prefix_graphics(tmp.path());
        assert_eq!(g.dxvk_version.as_deref(), Some("v3.1.1"));
        assert_eq!(g.vkd3d_proton_version.as_deref(), Some("3.1.0"));
        assert_eq!(
            g.dll_overrides.get("d3d9").map(String::as_str),
            Some("native")
        );
        let f = assess_graphics(&g);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rule_id, "graphics.native-override-without-native-dll");
        assert!(f[0].detail.as_deref().unwrap().starts_with("d3d9"));
    }

    #[test]
    fn proton_bundle_prefers_64_bit() {
        let tmp = tempfile::tempdir().unwrap();
        let d32 = tmp.path().join("files/lib/wine/dxvk/i386-windows");
        let d64 = tmp.path().join("files/lib/wine/dxvk/x86_64-windows");
        let v64 = tmp
            .path()
            .join("files/lib/wine/vkd3d-proton/x86_64-windows");
        for d in [&d32, &d64, &v64] {
            create_dir_all(d).unwrap();
        }
        write(d32.join("d3d11.dll"), pe(b"", b"\0DXVK: \0v1.0-32bit\0")).unwrap();
        write(d64.join("d3d11.dll"), pe(b"", b"\0DXVK: \0v2.4\0")).unwrap();
        write(
            v64.join("d3d12core.dll"),
            pe(b"", b"\0vkd3d\x002.13\0vkd3d-proton"),
        )
        .unwrap();
        let b = inspect_proton_bundle(tmp.path());
        assert_eq!(b.dxvk_version.as_deref(), Some("v2.4"));
        assert_eq!(b.vkd3d_proton_version.as_deref(), Some("2.13"));
    }
}
