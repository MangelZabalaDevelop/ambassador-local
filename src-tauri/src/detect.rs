//! Detecta sistema operativo, arquitectura y GPU NVIDIA para preseleccionar el equipo.

use crate::catalog::Catalog;
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

pub fn detect(catalog: &Catalog) -> System {
    let os = current_os().to_string();
    let arch = current_arch().to_string();
    let gpus = nvidia_smi();
    let suggested_hardware = catalog
        .hardware
        .iter()
        .find(|hw| {
            hw.os.contains(&os)
                && hw.arch.contains(&arch)
                && gpus.iter().any(|g| hw.gpu_match.iter().any(|m| g.name.to_uppercase().contains(&m.to_uppercase())))
        })
        .map(|hw| hw.id.clone());
    System { os, arch, gpus, suggested_hardware }
}
