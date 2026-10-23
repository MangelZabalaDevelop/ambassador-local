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
    while let Some(chunk) = stream.next().await {
        if progress.cancel.load(Ordering::Relaxed) {
            out.flush().await.ok();
            return Err("cancelled".into());
        }
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
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
    tokio::fs::rename(&part, dest).await.map_err(|e| e.to_string())?;
    tokio::fs::write(marker_path(dest), &file.sha256).await.map_err(|e| e.to_string())?;
    Ok(())
}
