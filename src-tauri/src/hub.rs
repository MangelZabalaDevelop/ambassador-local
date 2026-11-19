//! Pesos de las recetas en contenedor: la app los deja en la caché de Hugging Face del equipo tal como lo haría
//! `hf download <repo> --revision <commit>` (blobs/<huella> y snapshots/<commit>/<ruta> con enlaces relativos), que es
//! donde run.sh los busca. La lista de archivos, con su tamaño y su huella, la da Hugging Face para ese commit fijo.

use crate::catalog::repo_path_ok;
use crate::download::{self, Fingerprint, Progress};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const HUGGING_FACE: &str = "https://huggingface.co";

#[derive(Debug, Clone, PartialEq)]
pub struct HubFile {
    /// Ruta dentro del repo.
    pub path: String,
    pub size: u64,
    /// SHA256 si es LFS, si no el id de git. Es también el nombre del blob, como lo guarda huggingface_hub.
    pub oid: String,
    pub lfs: bool,
}

impl HubFile {
    fn fingerprint(&self) -> Fingerprint<'_> {
        if self.lfs {
            Fingerprint::Sha256(&self.oid)
        } else {
            Fingerprint::GitBlob(&self.oid)
        }
    }
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    oid: String,
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    oid: String,
    size: u64,
}

fn is_hex(s: &str, n: usize) -> bool {
    s.len() == n && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Archivos de una página del árbol del repo. Lo que no tenga una ruta o una huella válidas se rechaza: la ruta acaba
/// en el disco y la huella es el nombre del blob.
fn parse_tree(json: &str) -> Result<Vec<HubFile>, String> {
    let entries: Vec<Entry> = serde_json::from_str(json).map_err(|e| format!("Unexpected file list from Hugging Face: {e}"))?;
    let mut out = Vec::new();
    for e in entries.into_iter().filter(|e| e.kind == "file") {
        if !repo_path_ok(&e.path) {
            return Err(format!("Hugging Face listed a file path this app does not accept: {}", e.path));
        }
        let file = match e.lfs {
            Some(l) => HubFile { path: e.path, size: l.size, oid: l.oid, lfs: true },
            None => HubFile { path: e.path, size: e.size, oid: e.oid, lfs: false },
        };
        if !is_hex(&file.oid, if file.lfs { 64 } else { 40 }) {
            return Err(format!("Hugging Face gave no valid fingerprint for {}", file.path));
        }
        out.push(file);
    }
    Ok(out)
}

/// Solo los archivos de `include`, en ese orden; todos tienen que estar en el repo.
fn select(files: Vec<HubFile>, include: Option<&[String]>) -> Result<Vec<HubFile>, String> {
    let Some(include) = include else { return Ok(files) };
    include
        .iter()
        .map(|want| files.iter().find(|f| &f.path == want).cloned().ok_or_else(|| format!("{want} is not in the model repo at that revision.")))
        .collect()
}

/// La página siguiente del árbol, si la hay (cabecera `Link: <url>; rel="next"`), solo en el mismo sitio.
fn next_page(link: Option<&str>, base: &str) -> Option<String> {
    link?.split(',').find(|part| part.contains("rel=\"next\"")).and_then(|part| {
        let url = part.trim().strip_prefix('<')?.split_once('>')?.0;
        url.starts_with(&format!("{base}/")).then(|| url.to_string())
    })
}

/// Los archivos de `repo` en el commit `revision` (todos, o los de `include`).
pub async fn list(client: &reqwest::Client, base: &str, repo: &str, revision: &str, include: Option<&[String]>) -> Result<Vec<HubFile>, String> {
    let mut url = Some(format!("{base}/api/models/{repo}/tree/{revision}?recursive=true"));
    let mut files = Vec::new();
    while let Some(u) = url.take() {
        let fail = |e: String| format!("Could not get the list of files of {repo} from Hugging Face: {e}");
        let resp = client.get(&u).send().await.map_err(|e| fail(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(fail(format!("HTTP {}", resp.status())));
        }
        url = next_page(resp.headers().get(reqwest::header::LINK).and_then(|v| v.to_str().ok()), base);
        let body = resp.text().await.map_err(|e| fail(e.to_string()))?;
        files.extend(parse_tree(&body)?);
    }
    select(files, include)
}

/// La caché de Hugging Face, con las mismas reglas que config.sh: HF_HUB_CACHE, si no HF_HOME/hub, si no la de siempre.
pub fn hub_dir() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    var("HF_HUB_CACHE").or_else(|| var("HF_HOME").map(|h| h.join("hub"))).or_else(|| var("HOME").map(|h| h.join(".cache/huggingface/hub")))
}

fn model_dir(hub: &Path, repo: &str) -> PathBuf {
    hub.join(format!("models--{}", repo.replace('/', "--")))
}

fn snapshot_path(hub: &Path, repo: &str, revision: &str, file: &HubFile) -> PathBuf {
    model_dir(hub, repo).join("snapshots").join(revision).join(&file.path)
}

/// Enlace de snapshots/<commit>/<ruta> a su blob, relativo como los de huggingface_hub: sigue valiendo si la caché
/// se mueve o se monta en otra ruta (dentro del contenedor, por ejemplo).
#[cfg_attr(not(unix), allow(dead_code))]
fn link_target(file: &HubFile) -> String {
    format!("{}blobs/{}", "../".repeat(2 + file.path.matches('/').count()), file.oid)
}

/// ¿Está ya en la caché, entero? Se mira el tamaño, no se vuelve a calcular la huella de 160 GB en cada arranque
/// (huggingface_hub hace lo mismo).
pub fn present(hub: &Path, repo: &str, revision: &str, file: &HubFile) -> bool {
    std::fs::metadata(snapshot_path(hub, repo, revision, file)).is_ok_and(|m| m.is_file() && m.len() == file.size)
}

/// Bytes que faltan por bajar: lo que no está, menos lo que ya quedó a medias de un intento anterior.
pub fn bytes_missing(hub: &Path, repo: &str, files: &[&HubFile]) -> u64 {
    let blobs = model_dir(hub, repo).join("blobs");
    files.iter().map(|f| f.size.saturating_sub(std::fs::metadata(blobs.join(format!("{}.incomplete", f.oid))).map_or(0, |m| m.len()))).sum()
}

/// Espacio libre en el disco de la caché (o de la carpeta más cercana que exista).
#[cfg(target_os = "linux")]
pub fn free_space(hub: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let dir = hub.ancestors().find(|p| p.exists())?;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(path.as_ptr(), &mut st) } == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64)
}

#[cfg(not(target_os = "linux"))]
pub fn free_space(_hub: &Path) -> Option<u64> {
    None
}

/// Baja un archivo a blobs/ (reanudando un `.incomplete` de antes) y lo enlaza en la carpeta de la revisión.
pub async fn fetch(client: &reqwest::Client, base: &str, hub: &Path, repo: &str, revision: &str, file: &HubFile, progress: Progress<'_>) -> Result<(), String> {
    let blob = model_dir(hub, repo).join("blobs").join(&file.oid);
    if std::fs::metadata(&blob).is_ok_and(|m| m.len() == file.size) {
        (progress.on_bytes)(file.size);
    } else {
        let part = blob.with_file_name(format!("{}.incomplete", file.oid));
        let url = format!("{base}/{repo}/resolve/{revision}/{}", file.path);
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        download::fetch_part(client, &url, name, file.size, file.fingerprint(), &part, progress).await?;
        tokio::fs::rename(&part, &blob).await.map_err(|e| format!("Could not save {name}: {e}"))?;
    }
    link(hub, repo, revision, file)
}

#[cfg(unix)]
fn link(hub: &Path, repo: &str, revision: &str, file: &HubFile) -> Result<(), String> {
    let path = snapshot_path(hub, repo, revision, file);
    let fail = |e: std::io::Error| format!("Could not link {} into the Hugging Face cache: {e}", file.path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
    }
    if std::fs::symlink_metadata(&path).is_ok() {
        std::fs::remove_file(&path).map_err(fail)?;
    }
    std::os::unix::fs::symlink(link_target(file), &path).map_err(fail)
}

#[cfg(not(unix))]
fn link(_hub: &Path, _repo: &str, _revision: &str, _file: &HubFile) -> Result<(), String> {
    Err("Container recipes run only on Linux.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = r#"[
        {"type": "file", "oid": "1111111111111111111111111111111111111111", "size": 12, "path": "config.json"},
        {"type": "directory", "oid": "2222222222222222222222222222222222222222", "size": 0, "path": "tp1"},
        {"type": "file", "oid": "3333333333333333333333333333333333333333", "size": 135,
         "lfs": {"oid": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "size": 5000, "pointerSize": 135},
         "path": "tp1/model-1.safetensors"}
    ]"#;

    #[test]
    fn reads_the_tree_with_lfs_sizes_and_fingerprints() {
        let files = parse_tree(TREE).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], HubFile { path: "config.json".into(), size: 12, oid: "1".repeat(40), lfs: false });
        assert_eq!(files[1].size, 5000);
        assert!(files[1].lfs && files[1].oid == "a".repeat(64));
        assert_eq!(link_target(&files[0]), format!("../../blobs/{}", "1".repeat(40)));
        assert_eq!(link_target(&files[1]), format!("../../../blobs/{}", "a".repeat(64)));
    }

    #[test]
    fn rejects_paths_that_leave_the_model_folder_and_bad_fingerprints() {
        let bad_path = r#"[{"type": "file", "oid": "1111111111111111111111111111111111111111", "size": 1, "path": "../x"}]"#;
        assert!(parse_tree(bad_path).is_err());
        let bad_oid = r#"[{"type": "file", "oid": "../../etc", "size": 1, "path": "x"}]"#;
        assert!(parse_tree(bad_oid).is_err());
    }

    #[test]
    fn include_picks_files_and_reports_missing_ones() {
        let files = parse_tree(TREE).unwrap();
        let picked = select(files.clone(), Some(&["tp1/model-1.safetensors".to_string()])).unwrap();
        assert_eq!(picked.len(), 1);
        assert!(select(files.clone(), Some(&["nope.gguf".to_string()])).is_err());
        assert_eq!(select(files, None).unwrap().len(), 2);
    }

    #[test]
    fn follows_the_next_page_only_on_the_same_site() {
        let base = "https://huggingface.co";
        let link = r#"<https://huggingface.co/api/models/a/b/tree/c?recursive=true&cursor=xyz>; rel="next""#;
        assert_eq!(next_page(Some(link), base).as_deref(), Some("https://huggingface.co/api/models/a/b/tree/c?recursive=true&cursor=xyz"));
        assert_eq!(next_page(Some(r#"<https://evil.example/x>; rel="next""#), base), None);
        assert_eq!(next_page(None, base), None);
    }

    /// Contra Hugging Face de verdad: cada receta en contenedor de un equipo solo lista sus archivos, y con `include`
    /// suman justo el tamaño del catálogo. `cargo test catalog_repos_list -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn catalog_repos_list_on_hugging_face() {
        let cat = crate::catalog::get().unwrap();
        for m in &cat.models {
            for (hw, r) in &m.recipes {
                let (Some(v), true) = (&r.vllm, cat.hardware(hw).is_some_and(|h| h.units <= 1)) else { continue };
                let files = list(&download::client(), HUGGING_FACE, &v.repo, &v.revision, v.include.as_deref()).await.unwrap();
                let total: u64 = files.iter().map(|f| f.size).sum();
                println!("{} {hw}: {} files, {total} bytes (catalog {})", m.id, files.len(), v.size);
                if v.include.is_some() {
                    assert_eq!(total, v.size, "{} {hw}", m.id);
                }
            }
        }
    }

    /// De punta a punta contra un servidor local: blob con su huella de git, enlace relativo, y "ya está" después.
    #[cfg(unix)]
    #[tokio::test]
    async fn downloads_into_the_cache_layout() {
        use sha1::{Digest, Sha1};
        use std::sync::atomic::AtomicBool;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let body = b"{\"model_type\": \"test\"}\n".to_vec();
        let mut h = Sha1::new();
        h.update(format!("blob {}\0", body.len()));
        h.update(&body);
        let file = HubFile { path: "sub/config.json".into(), size: body.len() as u64, oid: hex::encode(h.finalize()), lfs: false };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let served = body.clone();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", served.len());
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&served).await;
            }
        });

        let hub = std::env::temp_dir().join(format!("al-hub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&hub);
        let (repo, rev) = ("org/model", "0123456789012345678901234567890123456789");
        assert!(!present(&hub, repo, rev, &file));
        let cancel = AtomicBool::new(false);
        let quiet = |_: u64| {};
        fetch(&download::client(), &base, &hub, repo, rev, &file, Progress { cancel: &cancel, on_bytes: &quiet }).await.unwrap();
        assert!(present(&hub, repo, rev, &file));
        let link = hub.join("models--org--model/snapshots").join(rev).join("sub/config.json");
        assert_eq!(std::fs::read_link(&link).unwrap(), PathBuf::from(link_target(&file)));
        assert_eq!(std::fs::read(&link).unwrap(), body);
        assert!(!hub.join("models--org--model/blobs").join(format!("{}.incomplete", file.oid)).exists());
    }
}
