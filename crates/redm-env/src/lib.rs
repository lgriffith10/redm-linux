//! Détection de l'environnement hôte.
//!
//! Collecte des faits (type d'hôte, noyau, distribution, CPU, mémoire, Vulkan,
//! Wine) puis les évalue en constats via [`assess`]. Toute la collecte passe
//! par [`SystemSource`] afin d'être testable sans l'hôte réel.

mod source;
mod vulkan;

pub use source::{CommandOutput, RealSystem, SystemSource};
pub use vulkan::{VulkanDevice, parse_vulkaninfo_summary};

use redm_core::{Finding, HostKind, Resolution, Severity};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Champs utiles de `/etc/os-release`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsRelease {
    pub id: Option<String>,
    pub version_id: Option<String>,
    pub pretty_name: Option<String>,
}

/// État de l'outil `vulkaninfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VulkanProbe {
    /// `vulkaninfo` absent : aucune conclusion possible sur Vulkan.
    ToolMissing,
    /// `vulkaninfo` a été exécuté.
    Ran,
}

/// Rapport brut de l'environnement. Aucune interprétation ici.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentReport {
    pub host: HostKind,
    pub kernel_release: Option<String>,
    pub os: OsRelease,
    pub cpu_model: Option<String>,
    pub logical_cpus: Option<usize>,
    pub memory_total_kib: Option<u64>,
    /// `/dev/dxg` présent : GPU para-virtualisé par Windows (WSL2).
    pub dxg_present: bool,
    /// `/dev/dri` présent : nœuds DRM, nécessaires à RADV (accès GPU natif).
    pub drm_present: bool,
    /// Manifeste ICD de Dozen (`dzn`) installé : seule voie Vulkan GPU sous WSL2.
    pub dozen_icd_present: bool,
    pub vulkan_probe: VulkanProbe,
    pub vulkan_devices: Vec<VulkanDevice>,
    pub wine_version: Option<String>,
}

/// Classe l'hôte à partir de la version du noyau et des indices WSL.
///
/// - WSL1 : noyau factice `x.y.z-NNNNN-Microsoft`.
/// - WSL2 : noyau `…-microsoft-standard-WSL2`, ou noyau personnalisé mais
///   variables/interop WSL présentes.
pub fn detect_host_kind(kernel_release: Option<&str>, wsl_signals: bool) -> HostKind {
    let Some(k) = kernel_release.map(str::trim).filter(|k| !k.is_empty()) else {
        return HostKind::Unknown;
    };
    let lower = k.to_ascii_lowercase();
    if lower.contains("microsoft") {
        if lower.contains("wsl2") || lower.contains("microsoft-standard") {
            HostKind::Wsl2
        } else {
            HostKind::Wsl1
        }
    } else if wsl_signals {
        HostKind::Wsl2
    } else {
        HostKind::NativeLinux
    }
}

pub fn parse_os_release(text: &str) -> OsRelease {
    let mut os = OsRelease::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = Some(v.trim().trim_matches('"').to_owned());
        match k.trim() {
            "ID" => os.id = v,
            "VERSION_ID" => os.version_id = v,
            "PRETTY_NAME" => os.pretty_name = v,
            _ => {}
        }
    }
    os
}

fn parse_cpuinfo(text: &str) -> (Option<String>, Option<usize>) {
    let model = text
        .lines()
        .find_map(|l| l.strip_prefix("model name"))
        .and_then(|r| r.split_once(':'))
        .map(|(_, v)| v.trim().to_owned());
    let count = text.lines().filter(|l| l.starts_with("processor")).count();
    (model, (count > 0).then_some(count))
}

fn parse_meminfo_total_kib(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

/// Emplacements usuels du manifeste ICD de Dozen (nommage Debian et Arch/Fedora).
const DOZEN_ICD_PATHS: &[&str] = &[
    "/usr/share/vulkan/icd.d/dzn_icd.json",
    "/usr/share/vulkan/icd.d/dzn_icd.x86_64.json",
    "/etc/vulkan/icd.d/dzn_icd.json",
    "/etc/vulkan/icd.d/dzn_icd.x86_64.json",
];

/// Constat « Vulkan logiciel uniquement », avec la cause la plus probable.
fn software_vulkan_finding(r: &EnvironmentReport) -> Finding {
    let f = Finding::new(
        "env.software-vulkan-only",
        Severity::Error,
        "Seul un rendu Vulkan logiciel est disponible (llvmpipe/lavapipe)",
        Resolution::LocalConfiguration,
    );
    match (r.host, r.drm_present, r.dozen_icd_present) {
        (HostKind::Wsl2, false, false) => f
            .with_detail(
                "WSL2 : pas de /dev/dri, donc RADV ne voit pas le GPU ; \
                 le pilote Dozen (dzn) n'est pas installé dans ce Mesa",
            )
            .with_advice(
                "Attendu sous WSL2. Le seul Vulkan matériel possible y passe par Dozen \
                 (Vulkan sur D3D12), lui-même non représentatif. Faire les tests de \
                 jeu sur un Linux natif.",
            ),
        (HostKind::Wsl2, false, true) => f
            .with_detail("WSL2 : le manifeste Dozen est installé mais aucun GPU n'est exposé")
            .with_advice("Vérifier `/dev/dxg` et le pilote GPU côté Windows."),
        (HostKind::NativeLinux, false, _) => f
            .with_detail("/dev/dri absent : aucun pilote DRM noyau chargé")
            .with_advice("Vérifier que le module `amdgpu` est chargé (`lsmod | grep amdgpu`)."),
        _ => f.with_advice(
            "Vérifier que `mesa-vulkan-drivers` est installé et que l'utilisateur \
             appartient aux groupes `video`/`render`.",
        ),
    }
}

/// Collecte l'environnement via la source donnée.
pub fn collect(src: &dyn SystemSource) -> EnvironmentReport {
    let read = |p: &str| src.read_to_string(Path::new(p));

    let kernel_release = read("/proc/sys/kernel/osrelease").map(|s| s.trim().to_owned());
    let wsl_signals = src.env_var("WSL_DISTRO_NAME").is_some()
        || src.env_var("WSL_INTEROP").is_some()
        || src.exists(Path::new("/proc/sys/fs/binfmt_misc/WSLInterop"));
    let host = detect_host_kind(kernel_release.as_deref(), wsl_signals);

    let os = read("/etc/os-release")
        .map(|t| parse_os_release(&t))
        .unwrap_or_default();
    let (cpu_model, logical_cpus) = read("/proc/cpuinfo")
        .map(|t| parse_cpuinfo(&t))
        .unwrap_or_default();
    let memory_total_kib = read("/proc/meminfo").and_then(|t| parse_meminfo_total_kib(&t));

    let (vulkan_probe, vulkan_devices) = match src.run("vulkaninfo", &["--summary"]) {
        None => (VulkanProbe::ToolMissing, Vec::new()),
        Some(out) => (VulkanProbe::Ran, parse_vulkaninfo_summary(&out.stdout)),
    };

    let wine_version = src
        .run("wine", &["--version"])
        .filter(|o| o.success)
        .map(|o| o.stdout.trim().to_owned())
        .filter(|s| !s.is_empty());

    EnvironmentReport {
        host,
        kernel_release,
        os,
        cpu_model,
        logical_cpus,
        memory_total_kib,
        dxg_present: src.exists(Path::new("/dev/dxg")),
        drm_present: src.exists(Path::new("/dev/dri")),
        dozen_icd_present: DOZEN_ICD_PATHS.iter().any(|p| src.exists(Path::new(p))),
        vulkan_probe,
        vulkan_devices,
        wine_version,
    }
}

/// Évalue un rapport d'environnement et produit des constats.
pub fn assess(r: &EnvironmentReport) -> Vec<Finding> {
    let mut out = Vec::new();

    match r.host {
        HostKind::NativeLinux => {}
        HostKind::Wsl2 => out.push(
            Finding::new(
                "env.wsl2-not-representative",
                Severity::Warning,
                "Exécution sous WSL2 : résultats non représentatifs d'un Linux natif",
                Resolution::LocalConfiguration,
            )
            .with_advice(
                "Utiliser WSL2 pour le développement uniquement ; valider toute \
                 conclusion de compatibilité sur un Linux natif.",
            ),
        ),
        HostKind::Wsl1 => out.push(
            Finding::new(
                "env.wsl1-unsupported",
                Severity::Error,
                "WSL1 détecté : pas d'accès GPU, non supporté",
                Resolution::LocalConfiguration,
            )
            .with_advice("Convertir la distribution : `wsl --set-version <distro> 2`."),
        ),
        HostKind::Unknown => out.push(Finding::new(
            "env.host-unknown",
            Severity::Warning,
            "Type d'hôte indéterminé",
            Resolution::Unknown,
        )),
    }

    match r.vulkan_probe {
        VulkanProbe::ToolMissing => out.push(
            Finding::new(
                "env.vulkaninfo-missing",
                Severity::Warning,
                "`vulkaninfo` introuvable : support Vulkan non vérifiable",
                Resolution::LocalConfiguration,
            )
            .with_advice("Installer `vulkan-tools` (Debian/Ubuntu)."),
        ),
        VulkanProbe::Ran if r.vulkan_devices.is_empty() => out.push(
            Finding::new(
                "env.no-vulkan-device",
                Severity::Error,
                "Aucun périphérique Vulkan : DXVK et VKD3D-Proton ne peuvent pas fonctionner",
                Resolution::LocalConfiguration,
            )
            .with_advice("Vérifier l'installation de Mesa (pilote RADV pour AMD)."),
        ),
        VulkanProbe::Ran => {
            let hw: Vec<_> = r.vulkan_devices.iter().filter(|d| !d.is_cpu()).collect();
            if hw.is_empty() {
                out.push(software_vulkan_finding(r));
            }
            for d in &hw {
                let name = d.name.as_deref().unwrap_or("?");
                if d.is_dozen() {
                    out.push(
                        Finding::new(
                            "env.vulkan-dozen",
                            Severity::Warning,
                            "Vulkan fourni par Dozen (traduction vers D3D12)",
                            Resolution::LocalConfiguration,
                        )
                        .with_detail(name)
                        .with_advice("Les performances et le comportement DXVK/VKD3D ne sont pas représentatifs."),
                    );
                }
                if d.api_major_minor().is_some_and(|v| v < (1, 3)) {
                    out.push(
                        Finding::new(
                            "env.vulkan-below-1.3",
                            Severity::Error,
                            "API Vulkan < 1.3 : requise par DXVK 2.x et VKD3D-Proton",
                            Resolution::LocalConfiguration,
                        )
                        .with_detail(format!(
                            "{name} : {}",
                            d.api_version.as_deref().unwrap_or("?")
                        ))
                        .with_advice("Mettre à jour Mesa / le pilote GPU."),
                    );
                }
            }
        }
    }

    if r.wine_version.is_none() {
        out.push(
            Finding::new(
                "env.wine-missing",
                Severity::Info,
                "Aucun `wine` système dans le PATH",
                Resolution::LocalConfiguration,
            )
            .with_advice("Sans importance si Proton est utilisé."),
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::source::fake::FakeSystem;
    use super::*;

    #[test]
    fn host_kind_detection() {
        assert_eq!(
            detect_host_kind(Some("6.18.33.2-microsoft-standard-WSL2"), false),
            HostKind::Wsl2
        );
        assert_eq!(
            detect_host_kind(Some("5.15.153.1-microsoft-standard-WSL2\n"), false),
            HostKind::Wsl2
        );
        assert_eq!(
            detect_host_kind(Some("4.4.0-19041-Microsoft"), false),
            HostKind::Wsl1
        );
        assert_eq!(
            detect_host_kind(Some("6.8.0-48-generic"), false),
            HostKind::NativeLinux
        );
        assert_eq!(detect_host_kind(Some("6.8.0-custom"), true), HostKind::Wsl2);
        assert_eq!(detect_host_kind(None, true), HostKind::Unknown);
        assert_eq!(detect_host_kind(Some("  "), false), HostKind::Unknown);
    }

    #[test]
    fn os_release_parsing() {
        let os = parse_os_release(
            "PRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\nID=ubuntu\nVERSION_ID=\"24.04\"\n",
        );
        assert_eq!(os.id.as_deref(), Some("ubuntu"));
        assert_eq!(os.version_id.as_deref(), Some("24.04"));
        assert_eq!(os.pretty_name.as_deref(), Some("Ubuntu 24.04.1 LTS"));
    }

    fn native_amd() -> FakeSystem {
        FakeSystem::default()
            .file("/proc/sys/kernel/osrelease", "6.8.0-48-generic\n")
            .file("/etc/os-release", "ID=ubuntu\nVERSION_ID=\"24.04\"\n")
            .file(
                "/proc/cpuinfo",
                "processor\t: 0\nmodel name\t: AMD Ryzen 9 5900X 12-Core Processor\n\
                 processor\t: 1\nmodel name\t: AMD Ryzen 9 5900X 12-Core Processor\n",
            )
            .file(
                "/proc/meminfo",
                "MemTotal:       32768000 kB\nMemFree: 1 kB\n",
            )
            .command(
                "vulkaninfo",
                "Devices:\nGPU0:\n\tapiVersion = 1.3.289\n\
                 \tdeviceType = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU\n\
                 \tdeviceName = AMD Radeon RX 7700 XT (RADV NAVI32)\n\tdriverName = radv\n",
            )
            .command("wine", "wine-9.0\n")
    }

    #[test]
    fn collect_native_machine() {
        let r = collect(&native_amd());
        assert_eq!(r.host, HostKind::NativeLinux);
        assert_eq!(
            r.cpu_model.as_deref(),
            Some("AMD Ryzen 9 5900X 12-Core Processor")
        );
        assert_eq!(r.logical_cpus, Some(2));
        assert_eq!(r.memory_total_kib, Some(32_768_000));
        assert_eq!(r.vulkan_devices.len(), 1);
        assert_eq!(r.wine_version.as_deref(), Some("wine-9.0"));
        assert!(!r.dxg_present);
        assert!(
            assess(&r).is_empty(),
            "machine native saine : aucun constat"
        );
    }

    #[test]
    fn collect_wsl2_without_tools() {
        let src = FakeSystem::default()
            .file(
                "/proc/sys/kernel/osrelease",
                "6.18.33.2-microsoft-standard-WSL2\n",
            )
            .file("/dev/dxg", "")
            .env("WSL_DISTRO_NAME", "Ubuntu");
        let r = collect(&src);
        assert_eq!(r.host, HostKind::Wsl2);
        assert!(r.dxg_present);
        assert_eq!(r.vulkan_probe, VulkanProbe::ToolMissing);
        let ids: Vec<_> = assess(&r).into_iter().map(|f| f.rule_id).collect();
        assert!(ids.contains(&"env.wsl2-not-representative".to_owned()));
        assert!(ids.contains(&"env.vulkaninfo-missing".to_owned()));
        assert!(ids.contains(&"env.wine-missing".to_owned()));
    }

    /// Reproduit l'hôte de développement réel (WSL2, Mesa 26.0.3, sans Dozen) :
    /// sortie `vulkaninfo --summary` capturée le 2026-11-21.
    #[test]
    fn wsl2_llvmpipe_only_explains_cause() {
        let src = FakeSystem::default()
            .file(
                "/proc/sys/kernel/osrelease",
                "6.18.33.2-microsoft-standard-WSL2\n",
            )
            .file("/dev/dxg", "")
            .command(
                "vulkaninfo",
                "Devices:\n========\nGPU0:\n\tapiVersion         = 1.4.335\n\
                 \tdeviceType         = PHYSICAL_DEVICE_TYPE_CPU\n\
                 \tdeviceName         = llvmpipe (LLVM 21.1.8, 256 bits)\n\
                 \tdriverID           = DRIVER_ID_MESA_LLVMPIPE\n\
                 \tdriverName         = llvmpipe\n\
                 \tdriverInfo         = Mesa 26.0.3-1ubuntu1 (LLVM 21.1.8)\n",
            );
        let r = collect(&src);
        assert!(!r.drm_present && !r.dozen_icd_present);
        let f = assess(&r)
            .into_iter()
            .find(|f| f.rule_id == "env.software-vulkan-only")
            .expect("constat attendu");
        assert!(f.detail.unwrap().contains("Dozen"));

        // Avec le manifeste Dozen, la cause rapportée change.
        let r = collect(&FakeSystem { ..src }.file("/usr/share/vulkan/icd.d/dzn_icd.json", "{}"));
        assert!(r.dozen_icd_present);
        let f = assess(&r)
            .into_iter()
            .find(|f| f.rule_id == "env.software-vulkan-only")
            .unwrap();
        assert!(f.detail.unwrap().contains("aucun GPU n'est exposé"));
    }

    #[test]
    fn assess_software_and_old_vulkan() {
        let mut r = collect(&native_amd());
        r.vulkan_devices = vec![VulkanDevice {
            device_type: Some("PHYSICAL_DEVICE_TYPE_CPU".into()),
            ..Default::default()
        }];
        let ids: Vec<_> = assess(&r).into_iter().map(|f| f.rule_id).collect();
        assert_eq!(ids, ["env.software-vulkan-only"]);

        r.vulkan_devices = vec![VulkanDevice {
            api_version: Some("1.2.198".into()),
            device_type: Some("PHYSICAL_DEVICE_TYPE_DISCRETE_GPU".into()),
            ..Default::default()
        }];
        let ids: Vec<_> = assess(&r).into_iter().map(|f| f.rule_id).collect();
        assert_eq!(ids, ["env.vulkan-below-1.3"]);

        r.vulkan_devices.clear();
        let ids: Vec<_> = assess(&r).into_iter().map(|f| f.rule_id).collect();
        assert_eq!(ids, ["env.no-vulkan-device"]);
    }
}
