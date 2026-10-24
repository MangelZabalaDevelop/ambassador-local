//! Detecta sistema operativo, arquitectura y GPU NVIDIA para preseleccionar el equipo.

use crate::catalog::{Catalog, Hardware};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    pub memory_mib: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct System {
    pub os: String,
    pub arch: String,
    pub gpus: Vec<Gpu>,
    pub suggested_hardware: Option<String>,
    /// Equipos del catálogo que coinciden con esta máquina: solo esos se pueden usar.
    pub detected_hardware: Vec<String>,
}

pub fn current_os() -> &'static str {
    std::env::consts::OS
}

pub fn current_arch() -> &'static str {
    std::env::consts::ARCH
}

fn nvidia_smi() -> Vec<Gpu> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]);
    crate::no_console_window(&mut cmd);
    let Ok(out) = cmd.output() else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let (name, mem) = line.rsplit_once(',')?;
            Some(Gpu { name: name.trim().to_string(), memory_mib: mem.trim().parse().ok() })
        })
        .collect()
}

fn matches(hw: &Hardware, os: &str, arch: &str, gpus: &[Gpu]) -> bool {
    // Varios equipos enlazados (GB10 ×2): además de la GPU, hace falta el cable ConnectX entre ellos.
    (hw.units <= 1 || crate::cluster::links() > 0)
        && hw.available
        && hw.os.iter().any(|o| o == os)
        && hw.arch.iter().any(|a| a == arch)
        && gpus.iter().any(|g| hw.gpu_match.iter().any(|m| g.name.to_uppercase().contains(&m.to_uppercase())))
}

/// ¿Esta máquina es ese equipo? Se vuelve a mirar al arrancar un modelo.
pub fn is_detected(hw: &Hardware) -> bool {
    matches(hw, current_os(), current_arch(), &nvidia_smi())
}

pub fn detect(catalog: &Catalog) -> System {
    let os = current_os().to_string();
    let arch = current_arch().to_string();
    let gpus = nvidia_smi();
    let detected_hardware: Vec<String> =
        catalog.hardware.iter().filter(|hw| matches(hw, &os, &arch, &gpus)).map(|hw| hw.id.clone()).collect();
    let suggested_hardware = detected_hardware.first().cloned();
    System { os, arch, gpus, suggested_hardware, detected_hardware }
}
