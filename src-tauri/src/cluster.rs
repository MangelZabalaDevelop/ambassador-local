//! GB10 ×2: el segundo equipo presta su memoria con `ggml-rpc-server` por el cable ConnectX.
//! Usa el SSH sin contraseña entre los dos equipos, el mismo que deja listo el asistente
//! de cluster de NVIDIA. El servidor RPC solo escucha en la IP del cable directo, nunca en
//! la red de la casa u oficina, y se apaga junto con el modelo.

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const RPC_PORT: u16 = 50052;
const RPC_NAME: &str = "ggml-rpc-server";
/// Carpeta del motor en el otro equipo, relativa a su $HOME.
const REMOTE_ROOT: &str = ".local/share/io.github.ambassadorlocal.app/rpc-engine";

/// Interfaz ConnectX (driver mlx5) con cable conectado.
fn mlx5_up(dev: &str) -> bool {
    let p = Path::new("/sys/class/net").join(dev);
    std::fs::read_link(p.join("device/driver")).is_ok_and(|d| d.ends_with("mlx5_core"))
        && std::fs::read_to_string(p.join("carrier")).is_ok_and(|c| c.trim() == "1")
}

/// Cuántos cables ConnectX están conectados.
pub fn links() -> usize {
    std::fs::read_dir("/sys/class/net")
        .map(|d| d.flatten().filter(|e| mlx5_up(&e.file_name().to_string_lossy())).count())
        .unwrap_or(0)
}

/// Mi IPv4 en una interfaz, con `ip` (siempre está en DGX OS).
fn own_ipv4(dev: &str) -> Option<Ipv4Addr> {
    let out = Command::new("ip").args(["-4", "-o", "addr", "show", "dev", dev]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let cidr = text.split_whitespace().skip_while(|w| *w != "inet").nth(1)?;
    cidr.split('/').next()?.parse().ok()
}

/// Equipos al otro lado de los cables ConnectX. Primero la tabla ARP; si está vacía
/// (enlace sin tráfico reciente), busca quién responde SSH en la misma subred.
pub fn peers() -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = Vec::new();
    let arp = std::fs::read_to_string("/proc/net/arp").unwrap_or_default();
    for f in arp.lines().skip(1).map(|l| l.split_whitespace().collect::<Vec<_>>()) {
        if f.len() >= 6 && f[2] == "0x2" && mlx5_up(f[5]) {
            if let Ok(ip) = f[0].parse() {
                if !out.contains(&ip) {
                    out.push(ip);
                }
            }
        }
    }
    if !out.is_empty() {
        return out;
    }
    let devs: Vec<String> = std::fs::read_dir("/sys/class/net")
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| mlx5_up(n)).collect())
        .unwrap_or_default();
    for me in devs.iter().filter_map(|d| own_ipv4(d)) {
        let [a, b, c, mine] = me.octets();
        let probes: Vec<_> = (1..=254u8)
            .filter(|h| *h != mine)
            .map(|h| {
                std::thread::spawn(move || {
                    let ip = Ipv4Addr::new(a, b, c, h);
                    TcpStream::connect_timeout(&SocketAddr::from((ip, 22)), Duration::from_millis(300)).ok().map(|_| ip)
                })
            })
            .collect();
        for ip in probes.into_iter().filter_map(|t| t.join().ok().flatten()) {
            if !out.contains(&ip) {
                out.push(ip);
            }
        }
    }
    out
}

fn ssh(peer: Ipv4Addr) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-n", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", &peer.to_string()]);
    c
}

fn run(mut cmd: Command, what: &str) -> Result<(), String> {
    let out = cmd.stderr(Stdio::piped()).output().map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{what}: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Arranca el servidor RPC en `peer` (copiando antes el motor si no lo tiene) y espera a que responda.
/// `server` es el llama-server local; el RPC está en la misma carpeta.
pub fn start_worker(peer: Ipv4Addr, engine_dir: &Path, server: &Path, lib_dirs: &[std::path::PathBuf], version: &str) -> Result<(), String> {
    let rel = |p: &Path| p.strip_prefix(engine_dir).map(|r| r.to_string_lossy().replace('\\', "/")).map_err(|_| "Engine layout not supported".to_string());
    let rpc = rel(&server.with_file_name(RPC_NAME))?;
    let remote = format!("{REMOTE_ROOT}/{version}");

    // ¿Ya tiene el motor? Si no, se copia por el cable (tar por SSH, sin archivos temporales).
    if run({ let mut c = ssh(peer); c.arg(format!("test -f {remote}/.ready")); c }, "check").is_err() {
        let mut tar = Command::new("tar").arg("-C").arg(engine_dir).args(["-cf", "-", "."]).stdout(Stdio::piped()).spawn().map_err(|e| format!("tar: {e}"))?;
        let mut copy = Command::new("ssh");
        copy.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5", &peer.to_string()])
            .arg(format!("rm -rf {remote} && mkdir -p {remote} && tar -C {remote} -xf - && touch {remote}/.ready"))
            .stdin(tar.stdout.take().ok_or("tar")?);
        let copied = run(copy, "Copying the engine to the second GB10");
        let _ = tar.wait();
        copied?;
    }

    let libs = lib_dirs.iter().map(|d| rel(d).map(|r| format!("$HOME/{remote}/{r}"))).collect::<Result<Vec<_>, _>>()?.join(":");
    let mut start = ssh(peer);
    start.arg(format!(
        "{} ; cd {remote} && LD_LIBRARY_PATH={libs} setsid nohup ./{rpc} -H {peer} -p {RPC_PORT} -c > rpc.log 2>&1 < /dev/null &",
        kill_cmd(peer)
    ));
    run(start, "Starting the second GB10")?;

    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&SocketAddr::from((peer, RPC_PORT)), Duration::from_secs(1)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(format!("The second GB10 ({peer}) did not answer. Check the technical log on it: ~/{remote}/rpc.log"))
}

/// `[g]` evita que pkill se mate a sí mismo: la shell remota lleva el patrón en su propia línea de comandos.
fn kill_cmd(peer: Ipv4Addr) -> String {
    format!("pkill -f '[{}]{} -H {peer} '", &RPC_NAME[..1], &RPC_NAME[1..])
}

pub fn stop_worker(peer: Ipv4Addr) {
    let mut c = ssh(peer);
    c.arg(kill_cmd(peer));
    let _ = c.output();
}
