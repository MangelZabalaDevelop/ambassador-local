//! Catálogo de recetas. Es solo datos: una receta nunca puede inyectar
//! argumentos arbitrarios, solo llenar los campos tipados de `Recipe`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

const BUNDLED: &str = include_str!("../../recipes/catalog.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    /// Ruta al JSON Schema; solo sirve para que el editor autocomplete.
    #[serde(rename = "$schema", default, skip_serializing)]
    #[allow(dead_code)]
    pub json_schema: Option<String>,
    pub schema: u32,
    pub engine: Engine,
    pub hardware: Vec<Hardware>,
    pub families: Vec<Family>,
    pub models: Vec<Model>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    pub name: String,
    pub version: String,
    pub builds: Vec<EngineBuild>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineBuild {
    pub os: String,
    pub arch: String,
    pub files: Vec<RemoteFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteFile {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl RemoteFile {
    /// Último segmento de la URL; ya validado como nombre de archivo seguro.
    pub fn file_name(&self) -> &str {
        self.url.rsplit('/').next().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hardware {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub gpu: String,
    pub memory_gb: u32,
    pub os: Vec<String>,
    pub arch: Vec<String>,
    pub gpu_match: Vec<String>,
    pub specs: Specs,
}

/// Ficha del producto que se muestra dentro de la app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Specs {
    pub model: String,
    pub summary: String,
    /// Cifras grandes de la ficha (ej. memoria de GPU).
    pub highlights: Vec<Spec>,
    pub details: Vec<Spec>,
    pub best_for: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Un laboratorio (Google, Qwen, NVIDIA…) y su serie de modelos.
pub struct Family {
    pub id: String,
    pub name: String,
    pub series: String,
    pub description: String,
    pub color: String,
    /// Laboratorio recomendado: se muestra con coronita.
    #[serde(default)]
    pub featured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub family: String,
    pub name: String,
    pub size: String,
    pub tagline: String,
    pub quant: String,
    /// Categoría de tamaño que ve el embajador.
    pub size_class: SizeClass,
    /// 1 a 5 bombillas: qué tan capaz es el modelo.
    pub intelligence: u8,
    /// Para qué sirve, en pocas palabras (1 a 3 etiquetas).
    pub best_for: Vec<String>,
    pub files: Vec<RemoteFile>,
    pub recipes: BTreeMap<String, Recipe>,
}

impl Model {
    /// Archivos que usa este equipo: los de su receta si los cambia, si no los del modelo.
    pub fn files_for(&self, hardware_id: &str) -> &[RemoteFile] {
        self.recipes.get(hardware_id).and_then(|r| r.files.as_deref()).unwrap_or(&self.files)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SizeClass {
    /// 4–14B parámetros.
    Small,
    /// 24–32B.
    Medium,
    /// ~70B, hasta 96 GB (RTX PRO 6000).
    Large,
    /// Cabe en dos GB10 / DGX Spark enlazadas.
    Xlarge,
}

/// Parámetros que una receta puede fijar. Todo lo demás lo decide la app
/// (host 127.0.0.1, puerto, --jinja).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// 1 a 5 rayos: qué tan rápido responde en este equipo. No es un argumento de llama.cpp.
    pub speed: u8,
    /// Promedio de tokens por segundo medido por el autor del catálogo en este
    /// equipo. Solo se muestra en la tabla; vacío = todavía sin medir.
    #[serde(default)]
    pub tokens_per_sec: Option<f32>,
    pub ctx_size: u32,
    /// Cuantización distinta para este equipo (p. ej. Q8 donde sobra memoria).
    /// Va junto con `files`; vacío = la del modelo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<RemoteFile>>,
    #[serde(default)]
    pub gpu_layers: Option<u32>,
    #[serde(default)]
    pub flash_attn: Option<bool>,
    #[serde(default)]
    pub mmap: Option<bool>,
    #[serde(default)]
    pub temp: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub top_k: Option<u32>,
    #[serde(default)]
    pub min_p: Option<f32>,
}

impl Recipe {
    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![
            "-c".into(),
            self.ctx_size.to_string(),
            "-ngl".into(),
            self.gpu_layers.unwrap_or(999).to_string(),
            "-fa".into(),
            if self.flash_attn.unwrap_or(true) { "on" } else { "off" }.into(),
        ];
        if self.mmap == Some(false) {
            args.push("--no-mmap".into());
        }
        let floats = [("--temp", self.temp), ("--top-p", self.top_p), ("--min-p", self.min_p)];
        for (flag, value) in floats {
            if let Some(v) = value {
                args.push(flag.into());
                args.push(v.to_string());
            }
        }
        if let Some(k) = self.top_k {
            args.push("--top-k".into());
            args.push(k.to_string());
        }
        args
    }
}

/// Orígenes desde donde se permite descargar. Los redirects se validan aparte.
pub const ALLOWED_ORIGINS: &[&str] = &["github.com", "huggingface.co"];

fn is_safe_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn validate_file(f: &RemoteFile) -> Result<(), String> {
    let url = url::Url::parse(&f.url).map_err(|e| format!("Invalid URL {}: {e}", f.url))?;
    if url.scheme() != "https" || !ALLOWED_ORIGINS.contains(&url.host_str().unwrap_or_default()) {
        return Err(format!("Origin not allowed: {}", f.url));
    }
    let name = f.file_name();
    if name.is_empty() || name.starts_with('.') || !name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err(format!("File name not allowed: {name}"));
    }
    if f.sha256.len() != 64 || !f.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(format!("Invalid sha256 for {name}"));
    }
    Ok(())
}

impl Catalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err(format!("Unsupported catalog version: {}", self.schema));
        }
        if !is_safe_id(&self.engine.version.to_lowercase()) {
            return Err("Invalid engine version".into());
        }
        for b in &self.engine.builds {
            b.files.iter().try_for_each(validate_file)?;
        }
        let hw_ids: HashSet<&str> = self.hardware.iter().map(|h| h.id.as_str()).collect();
        let fam_ids: HashSet<&str> = self.families.iter().map(|f| f.id.as_str()).collect();
        for id in hw_ids.iter().chain(fam_ids.iter()) {
            if !is_safe_id(id) {
                return Err(format!("Invalid id: {id}"));
            }
        }
        let mut model_ids = HashSet::new();
        for m in &self.models {
            if !is_safe_id(&m.id) || !model_ids.insert(m.id.as_str()) {
                return Err(format!("Invalid or duplicate model id: {}", m.id));
            }
            if !fam_ids.contains(m.family.as_str()) {
                return Err(format!("{}: unknown family {}", m.id, m.family));
            }
            if m.files.is_empty() {
                return Err(format!("{}: no files", m.id));
            }
            m.files.iter().try_for_each(validate_file)?;
            if !(1..=5).contains(&m.intelligence) {
                return Err(format!("{}: intelligence must be 1 to 5", m.id));
            }
            if m.best_for.is_empty() || m.best_for.len() > 3 || m.best_for.iter().any(|b| b.is_empty() || b.len() > 24) {
                return Err(format!("{}: best_for needs 1 to 3 short labels", m.id));
            }
            for (hw, r) in &m.recipes {
                if !(1..=5).contains(&r.speed) {
                    return Err(format!("{}: speed for {hw} must be 1 to 5", m.id));
                }
                if r.tokens_per_sec.is_some_and(|t| !(t > 0.0 && t < 10_000.0)) {
                    return Err(format!("{}: tokens_per_sec for {hw} out of range", m.id));
                }
                if !hw_ids.contains(hw.as_str()) {
                    return Err(format!("{}: unknown hardware {hw}", m.id));
                }
                if !(512..=262_144).contains(&r.ctx_size) {
                    return Err(format!("{}: ctx_size out of range", m.id));
                }
                if r.quant.is_some() != r.files.is_some() || r.quant.as_ref().is_some_and(|q| q.is_empty()) {
                    return Err(format!("{}: recipe for {hw} must set quant and files together", m.id));
                }
                if let Some(files) = &r.files {
                    if files.is_empty() {
                        return Err(format!("{}: recipe for {hw} has no files", m.id));
                    }
                    files.iter().try_for_each(validate_file)?;
                }
            }
        }
        Ok(())
    }

    pub fn hardware(&self, id: &str) -> Option<&Hardware> {
        self.hardware.iter().find(|h| h.id == id)
    }

    pub fn model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }
}

pub fn get() -> Result<&'static Catalog, String> {
    static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let c: Catalog = serde_json::from_str(BUNDLED).map_err(|e| format!("Invalid catalog: {e}"))?;
            c.validate()?;
            Ok(c)
        })
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_catalog_is_valid() {
        super::get().unwrap();
    }
}
