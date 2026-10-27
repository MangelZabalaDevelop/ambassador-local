//! Recetas en contenedor (vLLM en la T2, TensorFold en las dos GB10): la app lleva sus scripts dentro y los
//! deja en la carpeta de trabajo de la receta antes de arrancarla, así nadie tiene que abrir una terminal.

use std::path::{Path, PathBuf};

macro_rules! scripts {
    ($dir:literal: $($file:literal),+) => {
        ($dir, &[$(($file, include_str!(concat!("../../recipes/", $dir, "/", $file)))),+])
    };
}

/// `motor/receta` → archivos que necesita run.sh y stop.sh. Los pesos y la imagen los deja listos la revisión previa.
const SCRIPTS: &[(&str, &[(&str, &str)])] = &[
    scripts!("vllm/qwen38-flash-next-fp4-t2": "config.sh", "run.sh", "stop.sh", "engine.env"),
    scripts!("tensorfold/deepseek-v41-flash-1017": "config.sh", "run.sh", "stop.sh", "memwatch.sh"),
];

/// La carpeta de trabajo que usa config.sh por defecto: ~/.cache/ambassador-local/<motor>/<receta>.
pub fn work_dir(engine: &str, recipe: &str) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set.")?;
    Ok(Path::new(&home).join(".cache/ambassador-local").join(engine).join(recipe))
}

/// Escribe los scripts en <trabajo>/recipe y devuelve esa carpeta.
pub fn install(engine: &str, recipe: &str) -> Result<PathBuf, String> {
    let key = format!("{engine}/{recipe}");
    let (_, files) = SCRIPTS.iter().find(|(k, _)| *k == key).ok_or_else(|| format!("This app has no start scripts for {key}."))?;
    let dir = work_dir(engine, recipe)?.join("recipe");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    for (name, body) in files.iter() {
        let path = dir.join(name);
        // Un checkout de Windows podría traer CRLF: bash no los acepta.
        std::fs::write(&path, body.replace("\r\n", "\n")).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        #[cfg(unix)]
        if name.ends_with(".sh") {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_container_recipe_with_a_folder_has_its_scripts() {
        for m in &crate::catalog::get().unwrap().models {
            for r in m.recipes.values() {
                let key = match (&r.vllm, &r.tensorfold) {
                    (Some(v), _) => v.recipe.as_ref().map(|r| format!("vllm/{r}")),
                    (_, Some(t)) => Some(format!("tensorfold/{}", t.recipe)),
                    _ => None,
                };
                if let Some(key) = key {
                    assert!(SCRIPTS.iter().any(|(k, _)| *k == key), "{}: no embedded scripts for {key}", m.id);
                }
            }
        }
    }
}
