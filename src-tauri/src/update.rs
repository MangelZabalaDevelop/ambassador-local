//! Actualizaciones opcionales de la app (y con ella, del catálogo de modelos).
//! Solo se buscan en las releases del repo (endpoint fijo en tauri.conf.json) y
//! solo se instalan si vienen firmadas con nuestra clave: la pública va dentro
//! de la app y el plugin rechaza cualquier paquete que no la cumpla.
//! La interfaz no tiene permisos del updater; todo pasa por estos comandos.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::UpdaterExt;

use crate::AppState;

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    current: String,
    notes: Option<String>,
}

/// Solo pregunta si hay versión nueva; no descarga nada.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo { version: u.version.clone(), current: u.current_version.clone(), notes: u.body.clone() }))
}

/// Descarga, verifica la firma, instala y reinicia. Solo cuando el embajador lo pide.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state.inner.lock().unwrap().busy {
        return Err("Wait until the current download or start finishes.".into());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("You already have the latest version.")?;
    // Descargar y verificar antes de tocar nada: si falla, el modelo sigue corriendo.
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
    // El instalador reemplaza la app: el servidor no debe quedar huérfano.
    app.state::<AppState>().kill_now();
    update.install(bytes).map_err(|e| e.to_string())?;
    app.restart();
}
