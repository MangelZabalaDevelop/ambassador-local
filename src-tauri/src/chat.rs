//! El chat del modelo en marcha, en una ventana de la app: la página de chat de llama.cpp. llama-server la trae; para
//! vLLM y TensorFold, que solo tienen la API, la sirve el proxy de la app (proxy.rs). En el navegador esa página pide
//! la API key y hay que pegarla a mano; aquí se deja puesta antes de que cargue. El servidor la sigue exigiendo a
//! todos los demás.

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_opener::OpenerExt;
use url::{Origin, Url};

use crate::{proxy, AppState, Status};

/// Cada servidor tiene su ventana: "chat-<puerto>".
const PREFIX: &str = "chat-";

/// Guarda la clave en los ajustes de la página de chat de llama.cpp (localStorage `LlamaUi.config`; en versiones
/// anteriores `LlamaCppWebui.config`) antes de que cargue. Solo en el origen de nuestro servidor.
fn key_script(origin: &str, key: &str) -> String {
    let origin = serde_json::to_string(origin).unwrap_or_default();
    let key = serde_json::to_string(key).unwrap_or_default();
    format!(
        r#"(() => {{
  if (location.origin !== {origin}) return;
  for (const name of ["LlamaUi.config", "LlamaCppWebui.config"]) {{
    try {{
      let config = null;
      try {{ config = JSON.parse(localStorage.getItem(name) || "{{}}"); }} catch {{}}
      if (!config || typeof config !== "object" || Array.isArray(config)) config = {{}};
      config.apiKey = {key};
      localStorage.setItem(name, JSON.stringify(config));
    }} catch {{}}
  }}
}})();"#
    )
}

/// Lo que se queda en la ventana: la página del servidor (y sus blob:, about:blank de iframes).
fn inside(url: &Url, origin: &Origin) -> bool {
    url.scheme() == "about" || url.origin() == *origin
}

/// Los enlaces que pone el modelo en sus respuestas se abren en el navegador. Solo web y correo: nunca archivos ni
/// esquemas de otras apps.
fn open_outside(app: &AppHandle, url: &Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        let _ = app.opener().open_url(url.as_str(), None::<&str>);
    }
}

/// Abre el chat del modelo que está corriendo, o trae al frente el que ya está abierto.
/// Async: en Windows, crear una ventana desde un comando síncrono bloquea la app.
#[tauri::command]
pub async fn open_chat(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let status = state.inner.lock().unwrap().status.clone();
    let Some(Status::Ready { job, url, api_key, .. }) = status else {
        return Err("The model is not running.".into());
    };
    let model = crate::catalog::get()?.model(&job.model_id).ok_or("Unknown model")?;
    let recipe = model.recipes.get(&job.hardware_id).ok_or("Unknown model")?;
    // vLLM y TensorFold: la misma página, servida por la app delante de su API.
    let url = if model.in_container(&job.hardware_id) {
        let engine = if recipe.vllm.is_some() { "vLLM" } else { "TensorFold" };
        proxy::start(proxy::Target { upstream: url, key: api_key.clone(), model: model.id.clone(), ctx: recipe.ctx_size, engine }).await?
    } else {
        url
    };
    let url: Url = url.parse().map_err(|e| format!("Could not open the chat: {e}"))?;
    let label = format!("{PREFIX}{}", url.port_or_known_default().unwrap_or_default());
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.unminimize();
        let _ = window.show();
        return window.set_focus().map_err(|e| e.to_string());
    }
    close_all(&app);

    let origin = url.origin();
    let script = key_script(&origin.ascii_serialization(), &api_key);
    let (width, height) = size(&app);
    let (nav_app, nav_origin, popup_app) = (app.clone(), origin.clone(), app.clone());
    WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url))
        .title(format!("{} · Chat", model.name))
        .inner_size(width, height)
        .min_inner_size(480.0, 480.0)
        .center()
        .initialization_script(script)
        .on_navigation(move |u| {
            let stay = inside(u, &nav_origin);
            if !stay {
                open_outside(&nav_app, u);
            }
            stay
        })
        .on_new_window(move |u, _| {
            open_outside(&popup_app, &u);
            NewWindowResponse::Deny
        })
        .build()
        .map(|_| ())
        .map_err(|e| format!("Could not open the chat: {e}"))
}

/// 1100×800, o el 90 % de la pantalla de la app si es más chica (un portátil de 1366×768): que no quede cortada.
fn size(app: &AppHandle) -> (f64, f64) {
    let monitor = app.get_webview_window("main").and_then(|w| w.current_monitor().ok().flatten());
    match monitor {
        Some(m) => {
            let area = m.work_area().size.to_logical::<f64>(m.scale_factor());
            (1100f64.min(area.width * 0.9), 800f64.min(area.height * 0.9))
        }
        None => (1100.0, 800.0),
    }
}

/// Cierra los chats: solo sirven mientras su servidor está listo.
pub fn close_all(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(PREFIX) {
            let _ = window.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_server_and_its_frames_stay_in_the_window() {
        let origin = Url::parse("http://127.0.0.1:8080").unwrap().origin();
        let url = |s: &str| Url::parse(s).unwrap();
        assert!(inside(&url("http://127.0.0.1:8080/"), &origin));
        assert!(inside(&url("http://127.0.0.1:8080/#/chat/1?x=2"), &origin));
        assert!(inside(&url("blob:http://127.0.0.1:8080/6c1e0d1b-6bd9-4b0e-9a39-9b0b6f1c0d2a"), &origin));
        assert!(inside(&url("about:blank"), &origin));
        assert!(!inside(&url("http://127.0.0.1:8081/"), &origin));
        assert!(!inside(&url("http://localhost:8080/"), &origin));
        assert!(!inside(&url("https://example.com/"), &origin));
        assert!(!inside(&url("file:///etc/passwd"), &origin));
        assert!(!inside(&url("data:text/html,hi"), &origin));
    }

    #[test]
    fn the_key_script_only_writes_on_our_origin_and_escapes_its_values() {
        let s = key_script("http://127.0.0.1:8080", "sk-local-ab\"</script>");
        assert!(s.contains(r#"location.origin !== "http://127.0.0.1:8080""#));
        assert!(s.contains(r#"config.apiKey = "sk-local-ab\"</script>";"#));
        assert!(s.contains(r#"["LlamaUi.config", "LlamaCppWebui.config"]"#));
    }
}
