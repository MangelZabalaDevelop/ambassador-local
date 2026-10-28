//! Desinstalar desde la app. Para el modelo, quita la app como se instaló (instalador de Windows, paquete .deb o
//! AppImage) y, si el embajador lo pide, borra modelos y datos. Lo que solo se puede quitar con la app cerrada
//! (su ejecutable en Windows, los datos del navegador interno) lo termina un ayudante que espera a que salga.
//! La caché de Hugging Face y las imágenes de Docker no se tocan: otras herramientas pueden usarlas.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

/// Carpeta de trabajo de las recetas en contenedor (~/.cache/ambassador-local), ver recipe::work_dir.
const WORK_DIR: &str = "ambassador-local";

const NOT_INSTALLED: &str = "This copy of Ambassador Local was not installed with its installer, so there is nothing to uninstall. To remove it, delete the file or folder you run it from.";

/// Cómo se instaló esta copia.
#[derive(Debug, Clone, PartialEq)]
enum Method {
    /// Instalador NSIS: su uninstall.exe está junto al ejecutable.
    Windows { uninstaller: PathBuf },
    /// Paquete .deb (Ubuntu, DGX OS), instalado con apt.
    Deb { package: String },
    /// Un archivo .AppImage suelto.
    AppImage { file: PathBuf },
}

#[derive(Serialize)]
pub struct Info {
    /// "windows", "deb" o "appimage"; null si esta copia no se instaló (p. ej. un build de desarrollo).
    method: Option<&'static str>,
    /// Por qué no se puede desinstalar, cuando `method` es null.
    reason: Option<String>,
    /// El paquete .deb, para mostrarlo.
    package: Option<String>,
    /// Si quitarlo pide la contraseña de administrador (un .deb, salvo que la app ya corra como root).
    asks_password: bool,
    /// Lo que se libera al borrar modelos y datos.
    data_bytes: u64,
}

/// Un fallo, con el comando para terminarlo a mano cuando la app no puede (p. ej. sin ventana de contraseña).
#[derive(Debug, Serialize)]
pub struct Failure {
    message: String,
    command: Option<String>,
}

impl Failure {
    fn new(message: impl Into<String>) -> Self {
        Failure { message: message.into(), command: None }
    }
}

#[derive(Serialize)]
pub struct Done {
    /// true: todo salió bien y la app se cierra sola en un momento.
    closing: bool,
    /// Carpetas que no se pudieron borrar (p. ej. archivos que Docker dejó como root).
    leftovers: Vec<String>,
    /// Cómo borrarlas a mano.
    command: Option<String>,
}

fn detect() -> Result<Method, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if cfg!(windows) {
        let uninstaller = exe.with_file_name("uninstall.exe");
        return if uninstaller.is_file() { Ok(Method::Windows { uninstaller }) } else { Err(NOT_INSTALLED.into()) };
    }
    // Una AppImage se ejecuta montada: APPIMAGE dice dónde está el archivo de verdad.
    if let Some(file) = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|f| f.is_absolute() && f.is_file()) {
        return Ok(Method::AppImage { file });
    }
    deb_package(&exe).map(|package| Method::Deb { package }).ok_or_else(|| NOT_INSTALLED.into())
}

/// El paquete .deb dueño del ejecutable: `dpkg-query -S` responde "paquete: /usr/bin/...".
fn deb_package(exe: &Path) -> Option<String> {
    let out = Command::new("dpkg-query").arg("-S").arg(exe).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    parse_dpkg_owner(&String::from_utf8_lossy(&out.stdout))
}

fn parse_dpkg_owner(out: &str) -> Option<String> {
    let (package, path) = out.lines().next()?.split_once(": ")?;
    let ok = path.starts_with('/') && !package.is_empty() && package.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.:".contains(c));
    ok.then(|| package.to_string())
}

/// Carpetas de esta app: las de su identificador (datos, caché, configuración; dentro van modelos, motor, clave
/// y los datos del navegador interno) y la de trabajo de las recetas en contenedor.
fn data_dirs(app: &AppHandle) -> Vec<PathBuf> {
    let p = app.path();
    let mut dirs: Vec<PathBuf> = [p.app_local_data_dir(), p.app_data_dir(), p.app_cache_dir(), p.app_config_dir()].into_iter().flatten().collect();
    if cfg!(unix) {
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(Path::new(&home).join(".cache").join(WORK_DIR));
        }
    }
    let identifier = app.config().identifier.clone();
    dirs.sort();
    dirs.dedup();
    dirs.retain(|d| safe_to_delete(d, &identifier));
    dirs
}

/// Nunca se borra algo que no sea claramente nuestro: ruta absoluta, profunda, con nuestro nombre al final.
fn safe_to_delete(dir: &Path, identifier: &str) -> bool {
    dir.is_absolute() && dir.components().count() >= 4 && dir.file_name().is_some_and(|n| n == identifier || n == WORK_DIR)
}

/// Bytes que se liberan. En Linux, los archivos con más de un enlace (el checkpoint de TensorFold enlaza la caché de
/// Hugging Face) no cuentan: borrar ese enlace no libera nada.
fn freed_bytes(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if meta.is_dir() {
        return std::fs::read_dir(path).map(|d| d.flatten().map(|e| freed_bytes(&e.path())).sum()).unwrap_or(0);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            return 0;
        }
    }
    if meta.is_file() {
        meta.len()
    } else {
        0
    }
}

/// Borra lo que pueda y sigue con el resto. true si no quedó nada. En Windows reintenta un momento: un servidor
/// recién parado puede tener aún abiertos sus archivos.
fn remove_tree(path: &Path) -> bool {
    let tries = if cfg!(windows) { 10 } else { 1 };
    for attempt in 0..tries {
        remove_best_effort(path);
        if std::fs::symlink_metadata(path).is_err() {
            return true;
        }
        if attempt + 1 < tries {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    false
}

fn remove_best_effort(path: &Path) {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return };
    if meta.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for e in entries.flatten() {
                remove_best_effort(&e.path());
            }
        }
        let _ = std::fs::remove_dir(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Lo grande se borra con la app abierta, para poder decir qué no se pudo borrar: modelos, motor y descargas, y la
/// carpeta de trabajo de las recetas. El resto de las carpetas (pequeño, en uso por el navegador interno) lo
/// borra el ayudante al cerrar.
fn delete_heavy(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut leftovers = Vec::new();
    for dir in dirs {
        let targets: Vec<PathBuf> = if dir.file_name().is_some_and(|n| n == WORK_DIR) {
            vec![dir.clone()]
        } else {
            ["models", "engine", "downloads"].iter().map(|n| dir.join(n)).collect()
        };
        for t in targets {
            if !remove_tree(&t) {
                leftovers.push(t);
            }
        }
    }
    leftovers
}

/// Quita el paquete .deb. Como root, directo; si no, con la ventana de contraseña de Ubuntu (pkexec).
fn remove_deb(package: &str) -> Result<(), Failure> {
    let manual = format!("sudo apt remove {package}");
    let no_prompt = || Failure {
        message: "Ubuntu could not ask for your password here. Remove the app from a terminal:".into(),
        command: Some(manual.clone()),
    };
    let root = is_root();
    let apt = ["/usr/bin/apt-get", "remove", "-y", "-o", "DPkg::Lock::Timeout=120", package];
    let mut cmd = if root {
        let mut c = Command::new(apt[0]);
        c.args(&apt[1..]).env("DEBIAN_FRONTEND", "noninteractive");
        c
    } else {
        if !Path::new("/usr/bin/pkexec").is_file() {
            return Err(no_prompt());
        }
        let mut c = Command::new("/usr/bin/pkexec");
        c.args(["/usr/bin/env", "DEBIAN_FRONTEND=noninteractive"]).args(apt);
        c
    };
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| Failure { message: format!("Could not run apt: {e}"), command: Some(manual.clone()) })?;
    match out.status.code() {
        Some(0) => Ok(()),
        // pkexec: 126 = se cerró la ventana de contraseña; 127 = no hay quien la muestre o no se autorizó.
        Some(126) if !root => Err(Failure::new("Uninstall cancelled: the password window was closed. Nothing was removed.")),
        Some(127) if !root => Err(no_prompt()),
        _ => {
            let err = String::from_utf8_lossy(&out.stderr);
            let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error").trim().to_string();
            Err(Failure { message: format!("apt could not remove {package}: {last}"), command: Some(manual) })
        }
    }
}

#[cfg(target_os = "linux")]
fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(target_os = "linux"))]
fn is_root() -> bool {
    false
}

/// Windows: espera a que la app salga, corre el desinstalador en silencio y borra las carpetas de datos.
/// Va como -EncodedCommand (UTF-16): rutas con acentos o espacios llegan tal cual, sin problemas de comillas.
fn windows_helper(pid: u32, uninstaller: &Path, dirs: &[PathBuf]) -> String {
    let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
    let mut script = format!(
        "$ErrorActionPreference = 'SilentlyContinue'\nwhile (Get-Process -Id {pid}) {{ Start-Sleep -Milliseconds 300 }}\nStart-Process -FilePath {} -ArgumentList '/S' -Wait\n",
        quote(uninstaller)
    );
    // Reintentar un momento: los procesos de WebView2 pueden tardar en soltar sus archivos tras cerrar la app.
    for d in dirs {
        let d = quote(d);
        script.push_str(&format!(
            "for ($i = 0; $i -lt 20 -and (Test-Path -LiteralPath {d}); $i++) {{ Remove-Item -LiteralPath {d} -Recurse -Force; if (Test-Path -LiteralPath {d}) {{ Start-Sleep -Milliseconds 500 }} }}\n"
        ));
    }
    script
}

fn encode_powershell(script: &str) -> String {
    use base64::Engine;
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

/// Linux: espera a que la app salga y borra lo que queda (los datos del navegador interno, la AppImage).
fn unix_helper(pid: u32, paths: &[PathBuf]) -> Command {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(r#"while kill -0 "$0" 2>/dev/null; do sleep 0.3; done; sleep 0.5; rm -rf -- "$@""#)
        .arg(pid.to_string())
        .args(paths);
    cmd
}

fn spawn_helper(mut cmd: Command) -> Result<(), Failure> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    crate::no_console_window(&mut cmd);
    // Su propio grupo: que cerrar la app (o la terminal desde la que se abrió) no se lo lleve.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map(|_| ()).map_err(|e| Failure::new(format!("Could not start the uninstaller: {e}")))
}

#[tauri::command]
pub async fn uninstall_info(app: AppHandle) -> Result<Info, String> {
    let dirs = data_dirs(&app);
    let data_bytes = tauri::async_runtime::spawn_blocking(move || dirs.iter().map(|d| freed_bytes(d)).sum()).await.map_err(|e| e.to_string())?;
    Ok(match detect() {
        Ok(m) => Info {
            method: Some(match m {
                Method::Windows { .. } => "windows",
                Method::Deb { .. } => "deb",
                Method::AppImage { .. } => "appimage",
            }),
            reason: None,
            package: if let Method::Deb { package } = &m { Some(package.clone()) } else { None },
            asks_password: matches!(m, Method::Deb { .. }) && !is_root(),
            data_bytes,
        },
        Err(reason) => Info { method: None, reason: Some(reason), package: None, asks_password: false, data_bytes },
    })
}

/// Desinstala. Con `delete_data`, también modelos, motor, clave, ajustes y la carpeta de trabajo de las recetas.
#[tauri::command]
pub async fn uninstall(app: AppHandle, state: State<'_, AppState>, delete_data: bool) -> Result<Done, Failure> {
    if state.inner.lock().unwrap().busy {
        return Err(Failure::new("Wait until the current download or start finishes."));
    }
    let method = detect().map_err(Failure::new)?;
    let dirs = if delete_data { data_dirs(&app) } else { Vec::new() };
    let pid = std::process::id();

    // Primero parar el modelo: un servidor en marcha tiene abiertos sus archivos.
    state.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    let inner = state.inner.clone();
    tauri::async_runtime::spawn_blocking(move || crate::kill_now(&inner)).await.map_err(|e| Failure::new(e.to_string()))?;

    let leftovers = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<PathBuf>, Failure> {
        match &method {
            // El desinstalador cierra la app si sigue abierta: se lanza ya, pero espera a que salga.
            Method::Windows { uninstaller } => {
                let script = windows_helper(pid, uninstaller, &dirs);
                let mut cmd = Command::new("powershell.exe");
                cmd.args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-EncodedCommand", &encode_powershell(&script)]);
                spawn_helper(cmd)?;
                Ok(delete_heavy(&dirs))
            }
            // El paquete primero: si se cancela la contraseña, no se ha borrado nada.
            Method::Deb { package } => {
                remove_deb(package)?;
                let leftovers = delete_heavy(&dirs);
                if !dirs.is_empty() {
                    spawn_helper(unix_helper(pid, &dirs))?;
                }
                Ok(leftovers)
            }
            Method::AppImage { file } => {
                let mut paths = dirs.clone();
                paths.push(file.clone());
                spawn_helper(unix_helper(pid, &paths))?;
                Ok(delete_heavy(&dirs))
            }
        }
    })
    .await
    .map_err(|e| Failure::new(e.to_string()))??;

    if leftovers.is_empty() {
        // Un momento para que la ventana diga "Uninstalled" antes de cerrarse.
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            handle.exit(0);
        });
        return Ok(Done { closing: true, leftovers: Vec::new(), command: None });
    }
    let shown: Vec<String> = leftovers.iter().map(|p| p.display().to_string()).collect();
    let command = if cfg!(windows) {
        None
    } else {
        Some(format!("sudo rm -rf {}", leftovers.iter().map(|p| format!("'{}'", p.display().to_string().replace('\'', r"'\''"))).collect::<Vec<_>>().join(" ")))
    };
    Ok(Done { closing: false, leftovers: shown, command })
}

/// Cerrar la app (tras una desinstalación con restos que el embajador ya vio).
#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpkg_owner_is_parsed_and_validated() {
        assert_eq!(parse_dpkg_owner("ambassador-local: /usr/bin/ambassador-local\n").as_deref(), Some("ambassador-local"));
        assert_eq!(parse_dpkg_owner("ambassador-local:amd64: /usr/bin/ambassador-local").as_deref(), Some("ambassador-local:amd64"));
        assert_eq!(parse_dpkg_owner("dpkg-query: no path found matching pattern /x"), None);
        assert_eq!(parse_dpkg_owner("evil; rm -rf /: /usr/bin/x"), None);
        assert_eq!(parse_dpkg_owner(""), None);
    }

    #[test]
    fn only_our_own_folders_can_be_deleted() {
        let id = "io.github.ambassadorlocal.app";
        // Una ruta absoluta de verdad en cada sistema (en Windows, "/home/..." no lo es: falta la unidad).
        let home = Path::new(if cfg!(windows) { r"C:\Users\ana" } else { "/home/ana" });
        assert!(safe_to_delete(&home.join(".local/share/io.github.ambassadorlocal.app"), id));
        assert!(safe_to_delete(&home.join(".cache/ambassador-local"), id));
        assert!(!safe_to_delete(&home.join(".cache"), id));
        assert!(!safe_to_delete(&home.join(".cache/huggingface"), id));
        assert!(!safe_to_delete(Path::new(if cfg!(windows) { r"C:\ambassador-local" } else { "/ambassador-local" }), id));
        assert!(!safe_to_delete(Path::new("relative/x/y/ambassador-local"), id));
    }

    /// Lo que se libera: sin contar enlaces duros (borrarlos no libera nada) ni seguir enlaces simbólicos.
    #[test]
    fn freed_bytes_skips_hard_and_symbolic_links() {
        let root = std::env::temp_dir().join(format!("al-freed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("models/x")).unwrap();
        std::fs::write(root.join("models/x/a.gguf"), vec![0u8; 1000]).unwrap();
        std::fs::write(root.join("api-key.txt"), "k").unwrap();
        let outside = std::env::temp_dir().join(format!("al-freed-outside-{}", std::process::id()));
        std::fs::write(&outside, vec![0u8; 5000]).unwrap();
        #[cfg(unix)]
        {
            std::fs::hard_link(&outside, root.join("ckpt-link")).unwrap();
            std::os::unix::fs::symlink(&outside, root.join("sym")).unwrap();
        }
        assert_eq!(freed_bytes(&root), 1001);
        assert!(remove_tree(&root));
        assert!(outside.exists(), "deleting our folder must not touch what links point to");
        std::fs::remove_file(outside).unwrap();
    }

    /// Lo que no se puede borrar se reporta, y lo demás se borra igual.
    #[cfg(unix)]
    #[test]
    fn heavy_delete_reports_what_it_could_not_remove() {
        use std::os::unix::fs::PermissionsExt;
        if is_root() {
            return; // root borra todo: no hay forma de simular archivos ajenos
        }
        let data = std::env::temp_dir().join(format!("al-heavy-{}", std::process::id())).join("io.github.ambassadorlocal.app");
        std::fs::create_dir_all(data.join("models/m")).unwrap();
        std::fs::create_dir_all(data.join("engine/locked")).unwrap();
        std::fs::write(data.join("models/m/a.gguf"), "x").unwrap();
        std::fs::write(data.join("engine/locked/f"), "x").unwrap();
        std::fs::set_permissions(data.join("engine/locked"), std::fs::Permissions::from_mode(0o500)).unwrap();
        let left = delete_heavy(std::slice::from_ref(&data));
        std::fs::set_permissions(data.join("engine/locked"), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(left, vec![data.join("engine")]);
        assert!(!data.join("models").exists());
    }

    #[test]
    fn windows_helper_waits_uninstalls_and_quotes_paths() {
        let s = windows_helper(
            4242,
            Path::new(r"C:\Users\José O'Neil\AppData\Local\Ambassador Local\uninstall.exe"),
            &[PathBuf::from(r"C:\Users\José O'Neil\AppData\Local\io.github.ambassadorlocal.app")],
        );
        assert!(s.contains("Get-Process -Id 4242"));
        assert!(s.contains(r"Start-Process -FilePath 'C:\Users\José O''Neil\AppData\Local\Ambassador Local\uninstall.exe' -ArgumentList '/S' -Wait"));
        assert!(s.contains(r"Remove-Item -LiteralPath 'C:\Users\José O''Neil\AppData\Local\io.github.ambassadorlocal.app' -Recurse -Force"));
        assert!(s.contains(r"$i -lt 20 -and (Test-Path -LiteralPath 'C:\Users\José O''Neil\AppData\Local\io.github.ambassadorlocal.app')"));
        // PowerShell -EncodedCommand: base64 de UTF-16LE.
        use base64::Engine;
        let raw = base64::engine::general_purpose::STANDARD.decode(encode_powershell("é'")).unwrap();
        assert_eq!(raw, vec![0xE9, 0x00, 0x27, 0x00]);
    }

    /// El ayudante de Linux de verdad: espera a que el proceso salga y luego borra (y solo lo que se le pasa).
    #[cfg(unix)]
    #[test]
    fn unix_helper_waits_for_the_app_then_deletes() {
        let base = std::env::temp_dir().join(format!("al-helper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (target, keep) = (base.join("io.github.ambassadorlocal.app"), base.join("keep"));
        std::fs::create_dir_all(target.join("sub")).unwrap();
        std::fs::create_dir_all(&keep).unwrap();
        let mut app = Command::new("sleep").arg("1").spawn().unwrap();
        let mut helper = unix_helper(app.id(), std::slice::from_ref(&target)).spawn().unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(target.exists(), "deleted before the app closed");
        app.wait().unwrap();
        helper.wait().unwrap();
        assert!(!target.exists());
        assert!(keep.exists());
    }
}
