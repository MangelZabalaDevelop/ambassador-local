//! Instala el motor llama.cpp (binarios oficiales + runtime CUDA) en la carpeta de la app.

use std::fs;
use std::path::{Path, PathBuf};

pub struct EngineInstall {
    pub server: PathBuf,
    /// Carpetas con .dll/.so que el proceso necesita encontrar.
    pub lib_dirs: Vec<PathBuf>,
}

const SERVER_NAME: &str = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
const READY_MARKER: &str = ".ready";

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn is_lib(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    name.ends_with(".dll") || name.contains(".so")
}

pub fn locate(dir: &Path) -> Option<EngineInstall> {
    if !dir.join(READY_MARKER).exists() {
        return None;
    }
    let mut files = Vec::new();
    walk(dir, &mut files);
    let server = files.iter().find(|p| p.file_name().is_some_and(|n| n == SERVER_NAME))?.clone();
    let mut lib_dirs: Vec<PathBuf> = files.iter().filter(|p| is_lib(p)).filter_map(|p| p.parent().map(Path::to_path_buf)).collect();
    if let Some(d) = server.parent() {
        lib_dirs.push(d.to_path_buf());
    }
    lib_dirs.sort();
    lib_dirs.dedup();
    Some(EngineInstall { server, lib_dirs })
}

/// Extrae un .zip o .tar.gz dentro de `dest`, rechazando rutas que salgan de él.
pub fn extract(archive: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let name = archive.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let Some(rel) = entry.enclosed_name() else {
                return Err(format!("Unsafe path in {name}"));
            };
            let out = dest.join(rel);
            if entry.is_dir() {
                fs::create_dir_all(&out).map_err(|e| e.to_string())?;
                continue;
            }
            if let Some(p) = out.parent() {
                fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
        }
    } else if name.ends_with(".tar.gz") {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in tar.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            // unpack_in rechaza rutas absolutas y con "..".
            if !entry.unpack_in(dest).map_err(|e| e.to_string())? {
                return Err(format!("Unsafe path in {name}"));
            }
        }
    } else {
        return Err(format!("Unsupported format: {name}"));
    }
    Ok(())
}

pub fn mark_ready(dir: &Path) -> Result<(), String> {
    fs::write(dir.join(READY_MARKER), "ok").map_err(|e| e.to_string())
}
