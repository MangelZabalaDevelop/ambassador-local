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
    /// false = se muestra como "Coming soon" y no se puede elegir (p. ej. sin equipo para probar).
    #[serde(default = "yes")]
    pub available: bool,
    /// Equipos enlazados (2 = dos GB10 para modelos grandes). Solo cambia la ficha y la foto.
    #[serde(default = "one")]
    pub units: u32,
    /// Foto en public/hardware/<image>.webp; vacío = la del id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub specs: Specs,
}

fn yes() -> bool {
    true
}

fn one() -> u32 {
    1
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

    /// Archivo MTP aparte de la receta de este equipo, si lo usa.
    pub fn mtp_draft_for(&self, hardware_id: &str) -> Option<&RemoteFile> {
        self.recipes.get(hardware_id)?.mtp.as_ref()?.draft.as_ref()
    }

    /// Todo lo que hay que descargar para este equipo: el modelo y, si aplica, su archivo MTP.
    pub fn downloads_for(&self, hardware_id: &str) -> Vec<&RemoteFile> {
        self.files_for(hardware_id).iter().chain(self.mtp_draft_for(hardware_id)).collect()
    }

    pub fn vllm_for(&self, hardware_id: &str) -> Option<&Vllm> {
        self.recipes.get(hardware_id)?.vllm.as_ref()
    }

    pub fn tensorfold_for(&self, hardware_id: &str) -> Option<&TensorFold> {
        self.recipes.get(hardware_id)?.tensorfold.as_ref()
    }

    /// La receta corre en contenedores repartidos en las dos GB10 (vLLM o TensorFold), no con llama.cpp.
    pub fn in_container(&self, hardware_id: &str) -> bool {
        self.recipes.get(hardware_id).is_some_and(Recipe::in_container)
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
/// (host, puerto, --jinja).
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
    /// Decodificación especulativa con la cabeza MTP del modelo: misma calidad, bastante más rápido.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtp: Option<Mtp>,
    /// En vez de llama.cpp, vLLM en contenedor (solo GB10 ×2, recetas NVFP4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vllm: Option<Vllm>,
    /// En vez de llama.cpp, TensorFold en contenedor (solo GB10 ×2): imagen construida con la receta del repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tensorfold: Option<TensorFold>,
    /// Receta afinada a mano por el autor del catálogo: se ve con la etiqueta "Miguel Tuned".
    #[serde(default)]
    pub tuned: bool,
    /// Recomendada en este equipo: va primero en la tabla, con coronita.
    #[serde(default)]
    pub recommended: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mtp {
    /// Tokens que propone en cada paso (--spec-draft-n-max).
    pub draft_n_max: u32,
    /// GGUF aparte con la cabeza MTP (-md). Vacío = viene dentro del modelo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<RemoteFile>,
}

/// Modelo de vLLM: la imagen va fijada por digest y los pesos por commit, para que corra
/// exactamente lo que probamos. Cada GB10 necesita su copia de los pesos en la caché de Hugging Face.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vllm {
    /// `repositorio@sha256:<digest>`, p. ej. eugr/spark-vllm-b12x@sha256:03a9…
    pub image: String,
    /// `sha256:<id>` de la misma imagen. Una imagen copiada a la otra GB10 con
    /// docker save/load pierde el digest pero conserva este id (también es un hash del contenido).
    pub image_id: String,
    /// Repo de Hugging Face, p. ej. nvidia/GLM-5.3-Flash-FP4-Local.
    pub repo: String,
    /// Commit del repo (40 hex).
    pub revision: String,
    /// Bytes de los pesos en esa revisión, en cada GB10.
    pub size: u64,
    /// Carpeta con run.sh y stop.sh: recipes/vllm/<recipe>/. Obligatoria en un equipo solo (T2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
}

impl Vllm {
    /// Carpeta de la revisión en la caché de Hugging Face, relativa a `hub/`.
    pub fn snapshot_dir(&self) -> String {
        format!("models--{}/snapshots/{}", self.repo.replace('/', "--"), self.revision)
    }
}

/// TensorFold en contenedor. La imagen no está en ningún registro: cada GB10 la construye con
/// `recipes/tensorfold/<recipe>/` (fuentes fijadas por commit) y la etiqueta lleva el commit del motor.
/// `prepare.sh` deja los pesos en la caché de Hugging Face y los archivos de cada rank en su carpeta.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TensorFold {
    /// `nombre:tag` local, p. ej. ambassador-local/tensorfold-dsv41:6366d2c
    pub image: String,
    /// Commit de bertholomus/TensorFold que lleva la imagen (40 hex), en su etiqueta.
    pub engine_commit: String,
    /// Carpeta con Dockerfile y scripts: recipes/tensorfold/<recipe>/
    pub recipe: String,
    pub repo: String,
    pub revision: String,
    /// Bytes de los pesos en esa revisión, en cada GB10.
    pub size: u64,
}

impl TensorFold {
    pub fn snapshot_dir(&self) -> String {
        format!("models--{}/snapshots/{}", self.repo.replace('/', "--"), self.revision)
    }
}

fn hex(s: &str, n: usize) -> bool {
    s.len() == n && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

fn image_name_ok(s: &str) -> bool {
    !s.is_empty() && !s.starts_with(['-', '.', '/']) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._/-".contains(c))
}

fn repo_ok(repo: &str) -> bool {
    let part = |s: &str| !s.is_empty() && !s.starts_with(['-', '.']) && s.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    repo.split_once('/').is_some_and(|(org, name)| part(org) && part(name))
}

fn valid_vllm(v: &Vllm) -> bool {
    let image_ok = v.image.split_once("@sha256:").is_some_and(|(name, digest)| image_name_ok(name) && hex(digest, 64));
    let id_ok = v.image_id.strip_prefix("sha256:").is_some_and(|id| hex(id, 64));
    let recipe_ok = v.recipe.as_deref().map_or(true, is_safe_id);
    image_ok && id_ok && repo_ok(&v.repo) && hex(&v.revision, 40) && v.size > 0 && recipe_ok
}

fn valid_tensorfold(t: &TensorFold) -> bool {
    let tag_ok = |s: &str| !s.is_empty() && s.len() <= 128 && s.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    let image_ok = t.image.rsplit_once(':').is_some_and(|(name, tag)| image_name_ok(name) && tag_ok(tag));
    image_ok && hex(&t.engine_commit, 40) && is_safe_id(&t.recipe) && repo_ok(&t.repo) && hex(&t.revision, 40) && t.size > 0
}

impl Recipe {
    pub fn in_container(&self) -> bool {
        self.vllm.is_some() || self.tensorfold.is_some()
    }

    pub fn to_args(&self) -> Vec<String> {
        let mut args = vec![
            // Un solo usuario: un solo canal, con todo el contexto y toda la velocidad.
            "-np".into(),
            "1".into(),
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
        if let Some(m) = &self.mtp {
            args.extend(["--spec-type".into(), "draft-mtp".into(), "--spec-draft-n-max".into(), m.draft_n_max.to_string()]);
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
        for h in &self.hardware {
            if !(1..=8).contains(&h.units) || h.image.as_deref().is_some_and(|i| !is_safe_id(i)) {
                return Err(format!("{}: invalid units or image", h.id));
            }
        }
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
            // Un modelo solo de contenedores (vLLM, TensorFold) no tiene GGUF.
            if m.files.is_empty() && m.recipes.values().any(|r| !r.in_container()) {
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
                if r.vllm.as_ref().is_some_and(|v| !valid_vllm(v)) {
                    return Err(format!("{}: invalid vllm image, repo or revision for {hw}", m.id));
                }
                if r.tensorfold.as_ref().is_some_and(|t| !valid_tensorfold(t)) {
                    return Err(format!("{}: invalid tensorfold image, engine commit, recipe, repo or revision for {hw}", m.id));
                }
                if r.vllm.is_some() && r.tensorfold.is_some() {
                    return Err(format!("{}: the recipe for {hw} can't use vllm and tensorfold at once", m.id));
                }
                if r.in_container() {
                    let single = self.hardware(hw).is_some_and(|h| h.units < 2);
                    if single && r.tensorfold.is_some() {
                        return Err(format!("{}: tensorfold recipes are only for linked GB10 ({hw})", m.id));
                    }
                    // En un equipo solo la app no tiene cluster que revisar: hace falta el run.sh de la receta.
                    if single && r.vllm.as_ref().is_some_and(|v| v.recipe.is_none()) {
                        return Err(format!("{}: a vllm recipe for {hw} needs its recipe folder", m.id));
                    }
                    if r.files.is_some() || r.mtp.is_some() {
                        return Err(format!("{}: a vllm or tensorfold recipe for {hw} can't set GGUF files or mtp", m.id));
                    }
                }
                // vLLM y TensorFold llegan a 1M de contexto; llama.cpp se queda en 256K.
                let max_ctx = if r.in_container() { 1_048_576 } else { 262_144 };
                if !(512..=max_ctx).contains(&r.ctx_size) {
                    return Err(format!("{}: ctx_size out of range", m.id));
                }
                if r.quant.is_some() != r.files.is_some() || r.quant.as_ref().is_some_and(|q| q.is_empty()) {
                    return Err(format!("{}: recipe for {hw} must set quant and files together", m.id));
                }
                if let Some(mtp) = &r.mtp {
                    if !(1..=16).contains(&mtp.draft_n_max) {
                        return Err(format!("{}: mtp.draft_n_max for {hw} must be 1 to 16", m.id));
                    }
                    mtp.draft.iter().try_for_each(validate_file)?;
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

    /// La carpeta de cada receta de TensorFold existe y su config.sh fija lo mismo que el catálogo.
    #[test]
    fn tensorfold_recipes_match_their_folder() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../recipes/tensorfold");
        for m in &super::get().unwrap().models {
            for t in m.recipes.values().filter_map(|r| r.tensorfold.as_ref()) {
                let config = std::fs::read_to_string(root.join(&t.recipe).join("config.sh")).unwrap();
                for line in [
                    format!("IMAGE=\"${{IMAGE:-{}}}\"", t.image),
                    format!("ENGINE_COMMIT={}", t.engine_commit),
                    format!("REPO={}", t.repo),
                    format!("REVISION={}", t.revision),
                ] {
                    assert!(config.lines().any(|l| l.starts_with(&line)), "{}: config.sh has no {line}", m.id);
                }
                let docker = std::fs::read_to_string(root.join(&t.recipe).join("Dockerfile")).unwrap();
                assert!(docker.contains(&format!("ARG TF_REF={}", t.engine_commit)), "{}: Dockerfile builds another engine", m.id);
            }
        }
    }

    /// Igual para las recetas de vLLM con carpeta propia (las de un equipo solo).
    #[test]
    fn vllm_recipes_match_their_folder() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../recipes/vllm");
        for m in &super::get().unwrap().models {
            for v in m.recipes.values().filter_map(|r| r.vllm.as_ref()) {
                let Some(recipe) = &v.recipe else { continue };
                let config = std::fs::read_to_string(root.join(recipe).join("config.sh")).unwrap();
                for line in [format!("IMAGE=\"${{IMAGE:-{}}}\"", v.image), format!("REPO={}", v.repo), format!("REVISION={}", v.revision)] {
                    assert!(config.lines().any(|l| l.starts_with(&line)), "{}: config.sh has no {line}", m.id);
                }
                assert!(root.join(recipe).join("run.sh").is_file(), "{}: no run.sh", m.id);
            }
        }
    }
}
