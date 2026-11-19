//! Descargas verificadas: solo HTTPS, solo hosts permitidos (también en redirects),
//! reanudables, y el archivo final solo existe si su SHA256 coincide.

use crate::catalog::RemoteFile;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn host_allowed(host: &str) -> bool {
    const EXACT: &[&str] = &["github.com", "huggingface.co"];
    const SUFFIX: &[&str] = &[".githubusercontent.com", ".huggingface.co", ".hf.co"];
    EXACT.contains(&host) || SUFFIX.iter().any(|s| host.ends_with(s))
}

pub fn client() -> reqwest::Client {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        let ok = attempt.url().scheme() == "https" && attempt.url().host_str().is_some_and(host_allowed);
        if attempt.previous().len() > 10 {
            attempt.error("too many redirects")
        } else if ok {
            attempt.follow()
        } else {
            let msg = format!("redirect to a host that is not allowed: {}", attempt.url());
            attempt.error(msg)
        }
    });
    reqwest::Client::builder()
        .redirect(policy)
        .user_agent(concat!("AmbassadorLocal/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        // Una conexión que deja de mandar datos (wifi caída, proxy) termina en error en vez de colgar la descarga.
        .read_timeout(Duration::from_secs(60))
        .build()
        .expect("cliente HTTP")
}

fn marker_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".verified");
    p.into()
}

fn part_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".part");
    p.into()
}

/// ¿Está el archivo ya descargado y verificado?
pub fn is_verified(file: &RemoteFile, dest: &Path) -> bool {
    let size_ok = std::fs::metadata(dest).map(|m| m.len() == file.size).unwrap_or(false);
    let marker_ok = std::fs::read_to_string(marker_path(dest)).map(|s| s.trim() == file.sha256).unwrap_or(false);
    size_ok && marker_ok
}

pub struct Progress<'a> {
    pub cancel: &'a AtomicBool,
    pub on_bytes: &'a (dyn Fn(u64) + Send + Sync),
}

/// La huella que debe tener un archivo. Hugging Face da el SHA256 de los archivos grandes (LFS); de los pequeños
/// (config.json, el tokenizador) solo el id de git: SHA-1 de `blob <tamaño>\0` seguido del contenido.
#[derive(Debug, Clone, Copy)]
pub enum Fingerprint<'a> {
    Sha256(&'a str),
    GitBlob(&'a str),
}

impl Fingerprint<'_> {
    fn expected(&self) -> &str {
        match self {
            Fingerprint::Sha256(h) | Fingerprint::GitBlob(h) => h,
        }
    }

    fn hasher(&self, size: u64) -> Hasher {
        match self {
            Fingerprint::Sha256(_) => Hasher::Sha256(Sha256::new()),
            Fingerprint::GitBlob(_) => {
                let mut h = sha1::Sha1::new();
                h.update(format!("blob {size}\0"));
                Hasher::GitBlob(h)
            }
        }
    }
}

#[derive(Clone)]
enum Hasher {
    Sha256(Sha256),
    GitBlob(sha1::Sha1),
}

impl Hasher {
    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Sha256(h) => h.update(data),
            Hasher::GitBlob(h) => h.update(data),
        }
    }

    fn hex(self) -> String {
        match self {
            Hasher::Sha256(h) => hex::encode(h.finalize()),
            Hasher::GitBlob(h) => hex::encode(h.finalize()),
        }
    }
}

pub async fn fetch(client: &reqwest::Client, file: &RemoteFile, dest: &Path, progress: Progress<'_>) -> Result<(), String> {
    if is_verified(file, dest) {
        (progress.on_bytes)(file.size);
        return Ok(());
    }
    let part = part_path(dest);
    fetch_part(client, &file.url, file.file_name(), file.size, Fingerprint::Sha256(&file.sha256), &part, progress).await?;
    finish(file, &part, dest).await
}

/// Descarga `url` en `part`, reanudando lo que ya haya ahí. Solo devuelve Ok con el archivo entero y con su huella;
/// moverlo a su sitio es cosa de quien llama. `name` es para los mensajes.
pub async fn fetch_part(
    client: &reqwest::Client,
    url: &str,
    name: &str,
    size: u64,
    fingerprint: Fingerprint<'_>,
    part: &Path,
    progress: Progress<'_>,
) -> Result<(), String> {
    if let Some(dir) = part.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    }
    let mut hasher = fingerprint.hasher(size);
    let mut have: u64 = 0;

    // Reanudar: rehashear lo que ya está en disco. Con decenas de GB tarda; la barra avanza mientras tanto.
    if let Ok(mut f) = tokio::fs::File::open(part).await {
        let mut buf = vec![0u8; 1 << 20];
        let mut last_emit = Instant::now();
        loop {
            let n = f.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            have += n as u64;
            if progress.cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            if last_emit.elapsed() > Duration::from_millis(250) {
                (progress.on_bytes)(have.min(size));
                last_emit = Instant::now();
            }
        }
        if have > size {
            have = 0;
            hasher = fingerprint.hasher(size);
        }
    }
    // Ya está entero (se cortó justo antes de renombrarlo): pedir "bytes=<tamaño>-" daría 416 para siempre.
    if have == size {
        if hasher.clone().hex() == fingerprint.expected() {
            (progress.on_bytes)(have);
            return Ok(());
        }
        have = 0;
        hasher = fingerprint.hasher(size);
    }

    let mut req = client.get(url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await.map_err(|e| format!("Could not download {name}: {e}"))?;
    let status = resp.status();
    if have > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
        // El servidor no aceptó reanudar: empezar de cero.
        have = 0;
        hasher = fingerprint.hasher(size);
    }
    if !status.is_success() {
        return Err(format!("{name}: HTTP {status}"));
    }

    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(have > 0)
        .truncate(have == 0)
        .open(part)
        .await
        .map_err(|e| format!("Could not save {name}: {e}"))?;

    (progress.on_bytes)(have);
    let mut last_emit = Instant::now();
    let mut stream = resp.bytes_stream();
    loop {
        // Esperar el siguiente trozo sin dejar de mirar Cancel: con la red lenta o parada, Cancel responde igual.
        let next = tokio::time::timeout(Duration::from_millis(250), stream.next()).await;
        if progress.cancel.load(Ordering::Relaxed) {
            out.flush().await.ok();
            return Err("cancelled".into());
        }
        let Ok(next) = next else { continue };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|e| format!("Download of {name} interrupted ({e}). Press Try again to resume it."))?;
        have += chunk.len() as u64;
        if have > size {
            drop(out);
            tokio::fs::remove_file(part).await.ok();
            return Err(format!("{name}: larger than expected"));
        }
        hasher.update(&chunk);
        out.write_all(&chunk).await.map_err(|e| format!("Could not save {name}: {e}"))?;
        if last_emit.elapsed() > Duration::from_millis(250) {
            (progress.on_bytes)(have);
            last_emit = Instant::now();
        }
    }
    out.flush().await.map_err(|e| format!("Could not save {name}: {e}"))?;
    drop(out);
    (progress.on_bytes)(have);

    // La conexión se cerró antes de tiempo sin error: lo recibido sirve, se reanuda en el próximo intento.
    if have < size {
        return Err(format!("Download of {name} interrupted. Press Try again to resume it."));
    }
    if hasher.hex() != fingerprint.expected() {
        tokio::fs::remove_file(part).await.ok();
        let kind = match fingerprint {
            Fingerprint::Sha256(_) => "SHA256",
            Fingerprint::GitBlob(_) => "Git SHA-1",
        };
        return Err(format!("{name}: {kind} fingerprint does not match; the file was discarded"));
    }
    Ok(())
}

/// El .part ya verificado pasa a ser el archivo final, con su marca de verificado.
async fn finish(file: &RemoteFile, part: &Path, dest: &Path) -> Result<(), String> {
    tokio::fs::rename(part, dest).await.map_err(|e| e.to_string())?;
    tokio::fs::write(marker_path(dest), &file.sha256).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    #[derive(Clone, Copy)]
    enum Mode {
        /// Responde Range con 206, como GitHub y Hugging Face.
        Ranges,
        /// Ignora Range y manda todo con 200.
        Full,
        /// Manda la cabecera y unos bytes, y se queda callado.
        Stall,
        /// Anuncia y manda solo la primera mitad, y cierra sin error (un proxy que corta).
        Half,
    }

    /// Servidor HTTP mínimo en 127.0.0.1; cuenta las peticiones que recibe.
    async fn serve(body: Vec<u8>, mode: Mode) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/model.gguf", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                counter.fetch_add(1, Ordering::SeqCst);
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                    let from = req
                        .lines()
                        .find_map(|l| l.strip_prefix("range: bytes="))
                        .and_then(|r| r.trim_end_matches('-').trim().parse::<usize>().ok());
                    let (status, part) = match (mode, from) {
                        (Mode::Ranges, Some(f)) => ("206 Partial Content", &body[f..]),
                        (Mode::Half, _) => ("200 OK", &body[..body.len() / 2]),
                        _ => ("200 OK", &body[..]),
                    };
                    let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", part.len());
                    let _ = sock.write_all(head.as_bytes()).await;
                    if let Mode::Stall = mode {
                        let _ = sock.write_all(&part[..16]).await;
                        tokio::time::sleep(Duration::from_secs(3600)).await;
                    }
                    let _ = sock.write_all(part).await;
                });
            }
        });
        (url, hits)
    }

    fn remote(url: &str, body: &[u8]) -> RemoteFile {
        RemoteFile { url: url.into(), sha256: hex::encode(Sha256::digest(body)), size: body.len() as u64 }
    }

    fn body() -> Vec<u8> {
        (0..300_000u32).map(|i| (i % 251) as u8).collect()
    }

    async fn get(file: &RemoteFile, dest: &Path, cancel: &AtomicBool) -> Result<(), String> {
        let quiet = |_: u64| {};
        fetch(&client(), file, dest, Progress { cancel, on_bytes: &quiet }).await
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("al-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("model.gguf")
    }

    #[tokio::test]
    async fn downloads_verifies_and_marks() {
        let b = body();
        let (url, _) = serve(b.clone(), Mode::Ranges).await;
        let (file, dest) = (remote(&url, &b), scratch("fresh"));
        get(&file, &dest, &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b);
        assert!(is_verified(&file, &dest));
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn resumes_a_partial_download() {
        let b = body();
        let (url, _) = serve(b.clone(), Mode::Ranges).await;
        let (file, dest) = (remote(&url, &b), scratch("resume"));
        std::fs::write(part_path(&dest), &b[..123_456]).unwrap();
        get(&file, &dest, &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b);
    }

    #[tokio::test]
    async fn starts_over_when_the_server_ignores_range() {
        let b = body();
        let (url, _) = serve(b.clone(), Mode::Full).await;
        let (file, dest) = (remote(&url, &b), scratch("norange"));
        std::fs::write(part_path(&dest), &b[..1000]).unwrap();
        get(&file, &dest, &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b);
    }

    #[tokio::test]
    async fn a_complete_part_is_finished_without_asking_the_server() {
        let b = body();
        let (url, hits) = serve(b.clone(), Mode::Ranges).await;
        let (file, dest) = (remote(&url, &b), scratch("complete"));
        std::fs::write(part_path(&dest), &b).unwrap();
        get(&file, &dest, &AtomicBool::new(false)).await.unwrap();
        assert!(is_verified(&file, &dest));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_complete_but_corrupt_part_is_downloaded_again() {
        let b = body();
        let (url, _) = serve(b.clone(), Mode::Ranges).await;
        let (file, dest) = (remote(&url, &b), scratch("corrupt-part"));
        std::fs::write(part_path(&dest), vec![0u8; b.len()]).unwrap();
        get(&file, &dest, &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b);
    }

    #[tokio::test]
    async fn wrong_fingerprint_is_rejected_and_discarded() {
        let b = body();
        let (url, _) = serve(b.clone(), Mode::Ranges).await;
        let mut file = remote(&url, &b);
        file.sha256 = "0".repeat(64);
        let dest = scratch("badsha");
        let err = get(&file, &dest, &AtomicBool::new(false)).await.unwrap_err();
        assert!(err.contains("SHA256"), "{err}");
        assert!(!dest.exists() && !part_path(&dest).exists());
    }

    /// Los archivos pequeños de Hugging Face se verifican con su id de git (`git hash-object`).
    #[tokio::test]
    async fn verifies_git_blob_ids() {
        let b = b"hello
".to_vec();
        let (url, _) = serve(b.clone(), Mode::Ranges).await;
        let dest = scratch("gitblob");
        let part = part_path(&dest);
        let quiet = |_: u64| {};
        let cancel = AtomicBool::new(false);
        let c = client();
        let fetch = |oid: &'static str| fetch_part(&c, &url, "hello", 6, Fingerprint::GitBlob(oid), &part, Progress { cancel: &cancel, on_bytes: &quiet });
        let err = fetch("0000000000000000000000000000000000000000").await.unwrap_err();
        assert!(err.contains("Git SHA-1"), "{err}");
        fetch("ce013625030ba8dba906f756967f9e9ca394464a").await.unwrap();
        assert_eq!(std::fs::read(&part).unwrap(), b);
    }

    /// Antes, un corte limpio a medias se tomaba por archivo dañado y se borraba todo lo descargado.
    #[tokio::test]
    async fn a_clean_early_close_keeps_the_part_to_resume() {
        let b = body();
        let (half_url, _) = serve(b.clone(), Mode::Half).await;
        let dest = scratch("early-close");
        let err = get(&remote(&half_url, &b), &dest, &AtomicBool::new(false)).await.unwrap_err();
        assert!(err.contains("interrupted"), "{err}");
        assert_eq!(std::fs::metadata(part_path(&dest)).unwrap().len(), b.len() as u64 / 2);
        // El siguiente intento reanuda desde la mitad y termina.
        let (url, _) = serve(b.clone(), Mode::Ranges).await;
        get(&remote(&url, &b), &dest, &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b);
    }

    #[tokio::test]
    async fn cancel_works_while_the_connection_is_stalled() {
        let b = body();
        let (url, _) = serve(b.clone(), Mode::Stall).await;
        let (file, dest) = (remote(&url, &b), scratch("stall"));
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let err = tokio::time::timeout(Duration::from_secs(5), get(&file, &dest, &cancel)).await.expect("Cancel did not stop a stalled download");
        assert_eq!(err.unwrap_err(), "cancelled");
        assert!(started.elapsed() < Duration::from_secs(2));
        // Lo recibido queda para reanudar.
        assert_eq!(std::fs::metadata(part_path(&dest)).unwrap().len(), 16);
    }
}
