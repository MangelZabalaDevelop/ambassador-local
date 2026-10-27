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

pub async fn fetch(client: &reqwest::Client, file: &RemoteFile, dest: &Path, progress: Progress<'_>) -> Result<(), String> {
    if is_verified(file, dest) {
        (progress.on_bytes)(file.size);
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
    }
    let part = part_path(dest);
    let mut hasher = Sha256::new();
    let mut have: u64 = 0;

    // Reanudar: rehashear lo que ya está en disco.
    if let Ok(mut f) = tokio::fs::File::open(&part).await {
        let mut buf = vec![0u8; 1 << 20];
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
        }
        if have > file.size {
            have = 0;
            hasher = Sha256::new();
        }
    }
    // Ya está entero (se cortó justo antes de renombrarlo): pedir "bytes=<tamaño>-" daría 416 para siempre.
    if have == file.size {
        if hex::encode(hasher.clone().finalize()) == file.sha256 {
            (progress.on_bytes)(have);
            return finish(file, &part, dest).await;
        }
        have = 0;
        hasher = Sha256::new();
    }

    let mut req = client.get(&file.url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await.map_err(|e| format!("Could not download {}: {e}", file.file_name()))?;
    let status = resp.status();
    if have > 0 && status != reqwest::StatusCode::PARTIAL_CONTENT {
        // El servidor no aceptó reanudar: empezar de cero.
        have = 0;
        hasher = Sha256::new();
    }
    if !status.is_success() {
        return Err(format!("{}: HTTP {status}", file.file_name()));
    }

    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(have > 0)
        .truncate(have == 0)
        .open(&part)
        .await
        .map_err(|e| e.to_string())?;

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
        let chunk = chunk.map_err(|e| format!("Download of {} interrupted ({e}). Press Try again to resume it.", file.file_name()))?;
        have += chunk.len() as u64;
        if have > file.size {
            drop(out);
            tokio::fs::remove_file(&part).await.ok();
            return Err(format!("{}: larger than expected", file.file_name()));
        }
        hasher.update(&chunk);
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
        if last_emit.elapsed() > Duration::from_millis(250) {
            (progress.on_bytes)(have);
            last_emit = Instant::now();
        }
    }
    out.flush().await.map_err(|e| e.to_string())?;
    drop(out);
    (progress.on_bytes)(have);

    let digest = hex::encode(hasher.finalize());
    if have != file.size || digest != file.sha256 {
        tokio::fs::remove_file(&part).await.ok();
        return Err(format!("{}: SHA256 fingerprint does not match; the file was discarded", file.file_name()));
    }
    finish(file, &part, dest).await
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
