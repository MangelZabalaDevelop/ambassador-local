mod catalog;
mod cluster;
mod detect;
mod download;
mod engine;
mod lifetime;
mod update;
mod vllm;

use catalog::{Catalog, Model, RemoteFile};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tokio::io::{AsyncBufReadExt, BufReader};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Que los procesos hijos no abran una consola en Windows.
pub(crate) fn no_console_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum Status {
    Idle,
    Downloading { label: String, step: usize, steps: usize, downloaded: u64, total: u64 },
    Extracting,
    Starting { model_id: String },
    Ready { model_id: String, url: String, api_url: String, api_key: String },
    Error { message: String },
}

#[derive(Default)]
struct Inner {
    busy: bool,
    child: Option<tokio::process::Child>,
    /// GB10 ×2: el otro equipo con el servidor RPC encendido.
    peer: Option<std::net::Ipv4Addr>,
    status: Option<Status>,
    logs: VecDeque<String>,
}

#[derive(Default)]
struct AppState {
    inner: Arc<Mutex<Inner>>,
    cancel: Arc<AtomicBool>,
}

impl AppState {
    fn kill_now(&self) {
        let peer = {
            let mut g = self.inner.lock().unwrap();
            if let Some(mut child) = g.child.take() {
                let _ = child.start_kill();
            }
            g.peer.take()
        };
        if let Some(peer) = peer {
            cluster::stop_worker(peer);
        }
    }
}

fn emit_status(app: &AppHandle, inner: &Mutex<Inner>, status: Status) {
    inner.lock().unwrap().status = Some(status.clone());
    let _ = app.emit("status", status);
}

fn push_log(app: &AppHandle, inner: &Mutex<Inner>, line: String) {
    {
        let mut g = inner.lock().unwrap();
        if g.logs.len() >= 400 {
            g.logs.pop_front();
        }
        g.logs.push_back(line.clone());
    }
    let _ = app.emit("log", line);
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map_err(|e| e.to_string())
}

/// Clave del servidor local: se genera una vez y se reutiliza, para que Hermes
/// y otros clientes no tengan que reconfigurarse en cada arranque.
fn api_key_file(root: &Path) -> Result<(String, PathBuf), String> {
    let path = root.join("api-key.txt");
    if let Ok(saved) = std::fs::read_to_string(&path) {
        let key = saved.trim();
        if key.starts_with("sk-local-") && key.len() >= 40 {
            return Ok((key.to_string(), path));
        }
    }
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| format!("Could not create an API key: {e}"))?;
    let key = format!("sk-local-{}", hex::encode(bytes));
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{key}
")).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok((key, path))
}

fn model_dir(root: &Path, model: &Model) -> PathBuf {
    root.join("models").join(&model.id)
}

fn model_downloaded(root: &Path, model: &Model, hardware_id: &str) -> bool {
    let dir = model_dir(root, model);
    model.downloads_for(hardware_id).iter().all(|f| download::is_verified(f, &dir.join(f.file_name())))
}

#[tauri::command]
fn get_catalog() -> Result<Catalog, String> {
    catalog::get().cloned()
}

#[tauri::command]
async fn detect_system() -> Result<detect::System, String> {
    let c = catalog::get()?;
    tauri::async_runtime::spawn_blocking(move || detect::detect(c)).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn downloaded_models(app: AppHandle) -> Result<Vec<String>, String> {
    let root = data_dir(&app)?;
    // "modelo@equipo": cada receta puede usar otra cuantización (otros archivos).
    let mut out = Vec::new();
    for m in &catalog::get()?.models {
        // Los de vLLM viven en la caché de Hugging Face de cada GB10: los revisa check_cluster.
        for hw in m.recipes.keys().filter(|hw| m.vllm_for(hw).is_none() && model_downloaded(&root, m, hw)) {
            out.push(format!("{}@{hw}", m.id));
        }
    }
    Ok(out)
}

/// GB10 ×2 con vLLM: ¿están las dos listas para esta receta? Solo mira, no cambia nada.
#[tauri::command]
async fn check_cluster(hardware_id: String, model_id: String) -> Result<Vec<vllm::Check>, String> {
    let model = catalog::get()?.model(&model_id).ok_or("Unknown model")?;
    let v = model.vllm_for(&hardware_id).ok_or("This recipe does not use vLLM.")?.clone();
    tauri::async_runtime::spawn_blocking(move || vllm::verify(&v)).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn current_status(state: State<'_, AppState>) -> (Status, Vec<String>) {
    let g = state.inner.lock().unwrap();
    (g.status.clone().unwrap_or(Status::Idle), g.logs.iter().cloned().collect())
}

#[tauri::command]
fn cancel(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn stop_model(app: AppHandle, state: State<'_, AppState>) {
    state.kill_now();
    emit_status(&app, &state.inner, Status::Idle);
}

#[tauri::command]
async fn start_model(app: AppHandle, state: State<'_, AppState>, hardware_id: String, model_id: String) -> Result<(), String> {
    {
        let mut g = state.inner.lock().unwrap();
        if g.busy {
            return Err("A task is already running.".into());
        }
        g.busy = true;
        g.logs.clear();
    }
    state.kill_now();
    state.cancel.store(false, Ordering::Relaxed);

    let inner = state.inner.clone();
    let result = run_job(&app, &inner, &state.cancel, &hardware_id, &model_id).await;
    inner.lock().unwrap().busy = false;
    if let Err(message) = &result {
        state.kill_now();
        let message = if message == "cancelled" { "Cancelled.".to_string() } else { message.clone() };
        emit_status(&app, &inner, Status::Error { message });
    }
    result
}

async fn run_job(app: &AppHandle, inner: &Arc<Mutex<Inner>>, cancel: &AtomicBool, hardware_id: &str, model_id: &str) -> Result<(), String> {
    let cat = catalog::get()?;
    let (os, arch) = (detect::current_os(), detect::current_arch());
    let hw = cat.hardware(hardware_id).ok_or("Unknown hardware")?;
    if !hw.os.iter().any(|o| o == os) || !hw.arch.iter().any(|a| a == arch) {
        return Err(format!("{} does not run on this system ({os}/{arch}).", hw.name));
    }
    if !detect::is_detected(hw) {
        return Err(format!("{} was not detected on this machine.", hw.name));
    }
    let model = cat.model(model_id).ok_or("Unknown model")?;
    let recipe = model.recipes.get(hardware_id).ok_or("This model has no recipe for your hardware.")?;
    if recipe.vllm.is_some() {
        return Err("vLLM models can't be started from the app yet. Use the cluster check to see if both GB10 are ready.".into());
    }
    let build = cat
        .engine
        .builds
        .iter()
        .find(|b| b.os == os && b.arch == arch)
        .ok_or_else(|| format!("No engine available for {os}/{arch}."))?;

    let root = data_dir(app)?;
    let engine_dir = root.join("engine").join(&cat.engine.version);
    let mdir = model_dir(&root, model);
    let client = download::client();

    // Lista de descargas pendientes: motor (si falta) + modelo.
    let need_engine = engine::locate(&engine_dir).is_none();
    let mut queue: Vec<(String, &RemoteFile, PathBuf)> = Vec::new();
    if need_engine {
        let n = build.files.len();
        for (i, f) in build.files.iter().enumerate() {
            queue.push((format!("llama.cpp engine ({}/{n})", i + 1), f, root.join("downloads").join(f.file_name())));
        }
    }
    let files = model.files_for(hardware_id);
    let downloads = model.downloads_for(hardware_id);
    let n = downloads.len();
    for (i, f) in downloads.into_iter().enumerate() {
        let label = if n > 1 { format!("{} ({}/{n})", model.name, i + 1) } else { model.name.clone() };
        queue.push((label, f, mdir.join(f.file_name())));
    }

    let steps = queue.len();
    for (step, (label, file, dest)) in queue.iter().enumerate() {
        let (app2, inner2, label2, total) = (app.clone(), inner.clone(), label.clone(), file.size);
        let on_bytes = move |downloaded: u64| {
            emit_status(&app2, &inner2, Status::Downloading { label: label2.clone(), step: step + 1, steps, downloaded, total });
        };
        download::fetch(&client, file, dest, download::Progress { cancel, on_bytes: &on_bytes }).await?;
    }

    if need_engine {
        emit_status(app, inner, Status::Extracting);
        let archives: Vec<PathBuf> = build.files.iter().map(|f| root.join("downloads").join(f.file_name())).collect();
        let dir = engine_dir.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            let _ = std::fs::remove_dir_all(&dir);
            for a in &archives {
                engine::extract(a, &dir)?;
            }
            engine::mark_ready(&dir)?;
            for a in &archives {
                let _ = std::fs::remove_file(a);
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    let eng = engine::locate(&engine_dir).ok_or("llama-server was not found after installing the engine.")?;

    emit_status(app, inner, Status::Starting { model_id: model.id.clone() });

    // GB10 ×2: la otra GB10 presta su memoria por el cable directo.
    let mut rpc = Vec::new();
    if hw.units > 1 {
        let (dir, server, libs, version) = (engine_dir.clone(), eng.server.clone(), eng.lib_dirs.clone(), cat.engine.version.clone());
        let peer = tauri::async_runtime::spawn_blocking(move || -> Result<std::net::Ipv4Addr, String> {
            let peer = *cluster::peers().first().ok_or("The second GB10 was not found on the direct cable.")?;
            cluster::start_worker(peer, &dir, &server, &libs, &version)?;
            Ok(peer)
        })
        .await
        .map_err(|e| e.to_string())??;
        inner.lock().unwrap().peer = Some(peer);
        push_log(app, inner, format!("Second GB10 ready at {peer}:{}", cluster::RPC_PORT));
        rpc = vec!["--rpc".to_string(), format!("{peer}:{}", cluster::RPC_PORT)];
    }
    let port = (8080..8200)
        .find(|p| std::net::TcpListener::bind(("127.0.0.1", *p)).is_ok())
        .ok_or("No free ports.")?;

    // La clave va en un archivo, no en la línea de comandos, para que no se vea en la lista de procesos.
    let (api_key, key_path) = api_key_file(&root)?;
    let mut cmd = tokio::process::Command::new(&eng.server);
    cmd.arg("-m")
        .arg(mdir.join(files[0].file_name()))
        .args(["--host", "127.0.0.1", "--port", &port.to_string(), "--jinja", "--alias", &model.id])
        .arg("--api-key-file")
        .arg(&key_path)
        .args(recipe.to_args())
        .args(&rpc)
        .args(model.mtp_draft_for(hardware_id).map(|d| ["-md".into(), mdir.join(d.file_name()).into_os_string()]).into_iter().flatten())
        .current_dir(eng.server.parent().unwrap_or(&root))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let lib_var = if cfg!(windows) { "PATH" } else { "LD_LIBRARY_PATH" };
    let mut paths = eng.lib_dirs.clone();
    if let Some(existing) = std::env::var_os(lib_var) {
        paths.extend(std::env::split_paths(&existing));
    }
    cmd.env(lib_var, std::env::join_paths(paths).map_err(|e| e.to_string())?);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    lifetime::die_with_parent(&mut cmd);

    push_log(app, inner, format!("> {} {:?}", eng.server.display(), cmd.as_std().get_args().collect::<Vec<_>>()));
    let mut child = cmd.spawn().map_err(|e| format!("Could not start llama-server: {e}"))?;
    lifetime::bind_to_app(&child);
    for pipe in [child.stdout.take().map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), child.stderr.take().map(|s| Box::new(s) as _)]
        .into_iter()
        .flatten()
    {
        let (app, inner) = (app.clone(), inner.clone());
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                push_log(&app, &inner, line);
            }
        });
    }
    inner.lock().unwrap().child = Some(child);

    // Esperar a que /health responda o a que el proceso muera.
    let health = format!("http://127.0.0.1:{port}/health");
    let local = reqwest::Client::builder().timeout(Duration::from_secs(2)).build().map_err(|e| e.to_string())?;
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if cancel.load(Ordering::Relaxed) {
            if let Some(mut c) = inner.lock().unwrap().child.take() {
                let _ = c.start_kill();
            }
            return Err("cancelled".into());
        }
        if let Some(code) = exited(inner) {
            return Err(format!("llama-server exited ({code}). Check the technical log."));
        }
        if local.get(&health).send().await.is_ok_and(|r| r.status().is_success()) {
            break;
        }
    }
    let url = format!("http://127.0.0.1:{port}");
    emit_status(app, inner, Status::Ready { model_id: model.id.clone(), api_url: format!("{url}/v1"), url, api_key });

    // Vigilar el proceso: si muere, avisar a la interfaz.
    let (app, inner) = (app.clone(), inner.clone());
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if inner.lock().unwrap().child.is_none() {
                break;
            }
            if let Some(code) = exited(&inner) {
                emit_status(&app, &inner, Status::Error { message: format!("llama-server exited ({code}).") });
                break;
            }
        }
    });
    Ok(())
}

/// Si el hijo terminó, lo quita del estado y devuelve su código.
fn exited(inner: &Mutex<Inner>) -> Option<String> {
    let mut g = inner.lock().unwrap();
    let status = g.child.as_mut()?.try_wait().ok()??;
    g.child = None;
    Some(status.to_string())
}

#[cfg(test)]
mod smoke {
    //! Prueba real (descarga ~8 GB): `cargo test -- --ignored --nocapture`
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn engine_and_model_end_to_end() {
        let cat = catalog::get().unwrap();
        let (os, arch) = (detect::current_os(), detect::current_arch());
        let build = cat.engine.builds.iter().find(|b| b.os == os && b.arch == arch).unwrap();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("smoke");
        let engine_dir = root.join("engine").join(&cat.engine.version);
        let client = download::client();
        let cancel = AtomicBool::new(false);
        let quiet = |_: u64| {};
        let progress = || download::Progress { cancel: &cancel, on_bytes: &quiet };

        if engine::locate(&engine_dir).is_none() {
            for f in &build.files {
                let dest = root.join("downloads").join(f.file_name());
                download::fetch(&client, f, &dest, progress()).await.unwrap();
                engine::extract(&dest, &engine_dir).unwrap();
            }
            engine::mark_ready(&engine_dir).unwrap();
        }
        let eng = engine::locate(&engine_dir).expect("llama-server");
        println!("server: {}", eng.server.display());

        let model = &cat.models[0];
        let (hw, recipe) = model.recipes.iter().next().unwrap();
        let files = model.files_for(hw);
        let mdir = model_dir(&root, model);
        for f in model.downloads_for(hw) {
            download::fetch(&client, f, &mdir.join(f.file_name()), progress()).await.unwrap();
        }
        assert!(model_downloaded(&root, model, hw));

        let mut cmd = std::process::Command::new(&eng.server);
        cmd.arg("-m").arg(mdir.join(files[0].file_name()));
        cmd.args(["--host", "127.0.0.1", "--port", "8177", "--jinja"]).args(recipe.to_args());
        if let Some(d) = model.mtp_draft_for(hw) {
            cmd.arg("-md").arg(mdir.join(d.file_name()));
        }
        let lib_var = if cfg!(windows) { "PATH" } else { "LD_LIBRARY_PATH" };
        let mut paths = eng.lib_dirs.clone();
        paths.extend(std::env::split_paths(&std::env::var_os(lib_var).unwrap_or_default()));
        cmd.env(lib_var, std::env::join_paths(paths).unwrap());
        let mut child = cmd.spawn().unwrap();

        let local = reqwest::Client::new();
        let mut ok = false;
        for _ in 0..240 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if local.get("http://127.0.0.1:8177/health").send().await.is_ok_and(|r| r.status().is_success()) {
                ok = true;
                break;
            }
        }
        let reply = local
            .post("http://127.0.0.1:8177/v1/chat/completions")
            .json(&serde_json::json!({"messages":[{"role":"user","content":"Di hola en una palabra."}],"max_tokens":16}))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        child.kill().ok();
        assert!(ok, "health never responded");
        println!("respuesta: {reply}");
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_catalog,
            detect_system,
            downloaded_models,
            current_status,
            check_cluster,
            start_model,
            stop_model,
            cancel,
            update::check_update,
            update::install_update
        ])
        .build(tauri::generate_context!())
        .expect("error while starting the app")
        .run(|app, event| {
            // Cerrar la app = ya no se quiere inferencia: apagar el servidor.
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                app.state::<AppState>().kill_now();
            }
        });
}
