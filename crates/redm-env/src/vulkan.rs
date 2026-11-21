//! Analyse de la sortie de `vulkaninfo --summary`.

use serde::{Deserialize, Serialize};

/// Un périphérique physique Vulkan tel que rapporté par `vulkaninfo --summary`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VulkanDevice {
    pub name: Option<String>,
    pub device_type: Option<String>,
    pub api_version: Option<String>,
    pub driver_id: Option<String>,
    pub driver_name: Option<String>,
    pub driver_info: Option<String>,
    pub vendor_id: Option<String>,
}

impl VulkanDevice {
    /// Version d'API (majeure, mineure), si analysable.
    pub fn api_major_minor(&self) -> Option<(u32, u32)> {
        let v = self.api_version.as_deref()?;
        let mut it = v.split('.');
        let major = it.next()?.trim().parse().ok()?;
        let minor = it.next()?.trim().parse().ok()?;
        Some((major, minor))
    }

    /// Rendu logiciel (llvmpipe, lavapipe…).
    pub fn is_cpu(&self) -> bool {
        self.device_type
            .as_deref()
            .is_some_and(|t| t.contains("CPU"))
    }

    /// Dozen : Vulkan traduit vers Direct3D 12, typique de WSL2.
    pub fn is_dozen(&self) -> bool {
        self.driver_id
            .as_deref()
            .is_some_and(|d| d.contains("DOZEN"))
            || self.driver_name.as_deref() == Some("dzn")
    }
}

/// Extrait les périphériques de la section `Devices:` de `vulkaninfo --summary`.
pub fn parse_vulkaninfo_summary(text: &str) -> Vec<VulkanDevice> {
    let mut devices = Vec::new();
    let mut current: Option<VulkanDevice> = None;

    for raw in text.lines() {
        let line = raw.trim();
        // Début d'un bloc « GPU0: », « GPU1: »…
        if let Some(rest) = line.strip_prefix("GPU")
            && rest.ends_with(':')
            && rest[..rest.len() - 1].chars().all(|c| c.is_ascii_digit())
        {
            if let Some(d) = current.take() {
                devices.push(d);
            }
            current = Some(VulkanDevice::default());
            continue;
        }
        let Some(dev) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = Some(value.trim().to_owned());
        match key.trim() {
            "deviceName" => dev.name = value,
            "deviceType" => dev.device_type = value,
            "apiVersion" => dev.api_version = value,
            "driverID" => dev.driver_id = value,
            "driverName" => dev.driver_name = value,
            "driverInfo" => dev.driver_info = value,
            "vendorID" => dev.vendor_id = value,
            _ => {}
        }
    }
    if let Some(d) = current {
        devices.push(d);
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
==========
VULKANINFO
==========

Vulkan Instance Version: 1.3.275

Devices:
========
GPU0:
\tapiVersion         = 1.3.289
\tdriverVersion      = 24.2.8
\tvendorID           = 0x1002
\tdeviceID           = 0x7480
\tdeviceType         = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU
\tdeviceName         = AMD Radeon RX 7700 XT (RADV NAVI32)
\tdriverID           = DRIVER_ID_MESA_RADV
\tdriverName         = radv
\tdriverInfo         = Mesa 24.2.8
GPU1:
\tapiVersion         = 1.3.289
\tdeviceType         = PHYSICAL_DEVICE_TYPE_CPU
\tdeviceName         = llvmpipe (LLVM 19.1.1, 256 bits)
\tdriverID           = DRIVER_ID_MESA_LLVMPIPE
\tdriverName         = llvmpipe
";

    #[test]
    fn parses_two_devices() {
        let devs = parse_vulkaninfo_summary(SAMPLE);
        assert_eq!(devs.len(), 2);
        assert_eq!(
            devs[0].name.as_deref(),
            Some("AMD Radeon RX 7700 XT (RADV NAVI32)")
        );
        assert_eq!(devs[0].driver_name.as_deref(), Some("radv"));
        assert_eq!(devs[0].api_major_minor(), Some((1, 3)));
        assert!(!devs[0].is_cpu());
        assert!(devs[1].is_cpu());
    }

    #[test]
    fn empty_input_yields_nothing() {
        assert!(parse_vulkaninfo_summary("").is_empty());
        assert!(parse_vulkaninfo_summary("Devices:\n========\n").is_empty());
    }

    #[test]
    fn detects_dozen() {
        let d = VulkanDevice {
            driver_id: Some("DRIVER_ID_MESA_DOZEN".into()),
            ..Default::default()
        };
        assert!(d.is_dozen());
    }
}
