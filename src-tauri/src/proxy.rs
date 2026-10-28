//! El chat de llama.cpp para los modelos en contenedor (vLLM, TensorFold), que solo tienen la API. La app sirve en
//! 127.0.0.1 la misma página que trae llama-server (llama-ui/, de la versión del motor del catálogo) y le pasa el chat
//! al servidor del modelo, traduciendo lo poco que cambia:
//! - lo que solo tiene llama-server (/props, /build.json, /tools…) lo contesta aquí;
//! - de cada petición se quitan los campos propios de llama.cpp (vLLM rechaza `max_tokens: -1`);
//! - el `reasoning` de vLLM pasa a `reasoning_content`, como lo manda llama.cpp, y sus errores a la forma que la
//!   página muestra.
//!
//! Como llama-server, pide la API key (la ventana del chat la trae puesta, ver chat.rs).

use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{combinators::UnsyncBoxBody, BodyExt, Full, Limited, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::header::{HeaderValue, ACCEPT_ENCODING, AUTHORIZATION, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_TYPE};
use hyper::{Method, Request, Response, StatusCode};
use serde_json::{json, Map, Value};
use std::convert::Infallible;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

const INDEX: &[u8] = include_bytes!("../llama-ui/index.html");
const BUNDLE_JS: &[u8] = include_bytes!("../llama-ui/bundle.js.gz");
const BUNDLE_CSS: &[u8] = include_bytes!("../llama-ui/bundle.css.gz");
const FAVICON_ICO: &[u8] = include_bytes!("../llama-ui/favicon.ico");
const FAVICON_SVG: &[u8] = include_bytes!("../llama-ui/favicon.svg");
const FAVICON_DARK_ICO: &[u8] = include_bytes!("../llama-ui/favicon-dark.ico");
const FAVICON_DARK_SVG: &[u8] = include_bytes!("../llama-ui/favicon-dark.svg");
const MANIFEST: &[u8] = include_bytes!("../llama-ui/manifest.webmanifest");
const BUILD_JSON: &[u8] = include_bytes!("../llama-ui/build.json");
const VERSION_JSON: &[u8] = include_bytes!("../llama-ui/version.json");

/// Puertos del chat: siempre el mismo si está libre, porque la página guarda las conversaciones por origen.
const PORTS: std::ops::Range<u16> = 18080..18200;

/// Campos que entienden vLLM y los servidores compatibles con OpenAI; la página manda además los de llama.cpp.
const KEEP: &[&str] = &[
    "messages",
    "stream",
    "stream_options",
    "temperature",
    "top_p",
    "top_k",
    "min_p",
    "max_tokens",
    "max_completion_tokens",
    "stop",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "presence_penalty",
    "frequency_penalty",
    "repetition_penalty",
    "seed",
    "response_format",
    "chat_template_kwargs",
    "reasoning_effort",
    "logprobs",
    "top_logprobs",
];

/// El servidor del modelo al que se pasa el chat.
#[derive(Clone, PartialEq)]
pub struct Target {
    /// http://127.0.0.1:<puerto>, sin /v1.
    pub upstream: String,
    pub key: String,
    /// El nombre con el que lo sirve run.sh (SERVED_NAME): el id del modelo.
    pub model: String,
    pub ctx: u32,
    /// "vLLM" o "TensorFold", para la información del servidor que muestra la página.
    pub engine: &'static str,
}

struct Running {
    target: Target,
    url: String,
    stop: watch::Sender<bool>,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// Arranca el proxy para este servidor (o reutiliza el que ya corre) y devuelve su dirección.
pub async fn start(target: Target) -> Result<String, String> {
    {
        let running = RUNNING.lock().unwrap();
        if let Some(r) = running.as_ref().filter(|r| r.target == target) {
            return Ok(r.url.clone());
        }
    }
    stop();
    let mut listener = None;
    for port in PORTS {
        if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
            listener = Some(l);
            break;
        }
    }
    let listener = listener.ok_or("No free port for the chat.")?;
    let url = format!("http://127.0.0.1:{}", listener.local_addr().map_err(|e| e.to_string())?.port());
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
    let (stop_tx, stop_rx) = watch::channel(false);
    let shared = Arc::new((target.clone(), client));
    tauri::async_runtime::spawn(serve(listener, shared, stop_rx));
    *RUNNING.lock().unwrap() = Some(Running { target, url: url.clone(), stop: stop_tx });
    Ok(url)
}

/// Lo para: el modelo se paró o se arrancó otro. Corta también los chats a medio responder.
pub fn stop() {
    if let Some(r) = RUNNING.lock().unwrap().take() {
        let _ = r.stop.send(true);
    }
}

async fn serve(listener: tokio::net::TcpListener, shared: Arc<(Target, reqwest::Client)>, mut stop: watch::Receiver<bool>) {
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                let (shared, mut stop) = (shared.clone(), stop.clone());
                tauri::async_runtime::spawn(async move {
                    let service = hyper::service::service_fn(move |req| handle(req, shared.clone()));
                    let conn = hyper::server::conn::http1::Builder::new().serve_connection(hyper_util::rt::TokioIo::new(stream), service);
                    tokio::select! {
                        _ = conn => {}
                        _ = stop.changed() => {}
                    }
                });
            }
        }
    }
}

type Body = UnsyncBoxBody<Bytes, Infallible>;

fn full(status: StatusCode, content_type: &'static str, body: impl Into<Bytes>) -> Response<Body> {
    let mut res = Response::new(Full::new(body.into()).boxed_unsync());
    *res.status_mut() = status;
    res.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    res
}

fn json_response(status: StatusCode, v: &Value) -> Response<Body> {
    full(status, "application/json; charset=utf-8", v.to_string())
}

/// Los errores con la forma de llama-server, que es la que la página sabe mostrar.
fn error(status: StatusCode, kind: &str, message: &str) -> Response<Body> {
    json_response(status, &json!({ "error": { "code": status.as_u16(), "message": message, "type": kind } }))
}

/// Un archivo de la página. Los grandes van comprimidos; si el navegador no acepta gzip, se descomprimen.
fn asset(req: &Request<Incoming>, content_type: &'static str, bytes: &'static [u8], gzipped: bool) -> Response<Body> {
    let accepts_gzip = req.headers().get(ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("gzip"));
    let mut res = if gzipped && !accepts_gzip {
        let mut plain = Vec::new();
        let _ = flate2::read::GzDecoder::new(bytes).read_to_end(&mut plain);
        full(StatusCode::OK, content_type, plain)
    } else {
        full(StatusCode::OK, content_type, Bytes::from_static(bytes))
    };
    if gzipped && accepts_gzip {
        res.headers_mut().insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
    }
    // Los nombres de bundle llevan su hash; la página y las versiones se piden de nuevo cada vez.
    let cache = if gzipped { "public, max-age=31536000, immutable" } else { "no-cache" };
    res.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static(cache));
    res
}

/// Como llama-server: `Authorization: Bearer <clave>` o `X-Api-Key: <clave>`.
fn authorized(req: &Request<Incoming>, key: &str) -> bool {
    let header = |name| req.headers().get(name).and_then(|v: &HeaderValue| v.to_str().ok());
    header(AUTHORIZATION.as_str()).map(|v| v.strip_prefix("Bearer ").unwrap_or(v)).or_else(|| header("x-api-key")) == Some(key)
}

async fn handle(req: Request<Incoming>, shared: Arc<(Target, reqwest::Client)>) -> Result<Response<Body>, Infallible> {
    let (target, client) = (&shared.0, &shared.1);
    let path = req.uri().path().to_string();
    let get = req.method() == Method::GET;
    // Lo público, como en llama-server: la página, health y la lista de modelos.
    let res = match path.as_str() {
        "/" | "/index.html" if get => asset(&req, "text/html; charset=utf-8", INDEX, false),
        p if get && p.starts_with("/_app/immutable/bundle.") && p.ends_with(".js") => asset(&req, "application/javascript", BUNDLE_JS, true),
        p if get && p.starts_with("/_app/immutable/assets/bundle.") && p.ends_with(".css") => asset(&req, "text/css", BUNDLE_CSS, true),
        "/favicon.ico" if get => asset(&req, "image/x-icon", FAVICON_ICO, false),
        "/favicon.svg" if get => asset(&req, "image/svg+xml", FAVICON_SVG, false),
        "/favicon-dark.ico" if get => asset(&req, "image/x-icon", FAVICON_DARK_ICO, false),
        "/favicon-dark.svg" if get => asset(&req, "image/svg+xml", FAVICON_DARK_SVG, false),
        "/manifest.webmanifest" if get => asset(&req, "application/manifest+json", MANIFEST, false),
        "/build.json" if get => asset(&req, "application/json", BUILD_JSON, false),
        "/_app/version.json" if get => asset(&req, "application/json", VERSION_JSON, false),
        "/health" | "/v1/health" if get => json_response(StatusCode::OK, &json!({ "status": "ok" })),
        "/models" | "/v1/models" if get => json_response(StatusCode::OK, &models(target)),
        _ if !authorized(&req, &target.key) => error(StatusCode::UNAUTHORIZED, "authentication_error", "Invalid API Key"),
        "/props" if get => json_response(StatusCode::OK, &props(target)),
        // Herramientas del servidor (--tools en llama-server): aquí no hay, como en un llama-server sin ellas.
        "/tools" => error(StatusCode::FORBIDDEN, "feature_disabled", "this feature is disabled"),
        // Reanudar respuestas tras recargar: solo lo tiene llama-server.
        "/v1/streams/lookup" if req.method() == Method::POST => json_response(StatusCode::OK, &json!([])),
        "/v1/chat/completions" | "/chat/completions" if req.method() == Method::POST => chat(req, target, client).await,
        _ => error(StatusCode::NOT_FOUND, "not_found_error", "File Not Found"),
    };
    Ok(res)
}

fn models(t: &Target) -> Value {
    json!({ "object": "list", "data": [{ "id": t.model, "object": "model", "owned_by": t.engine, "max_model_len": t.ctx }] })
}

/// Lo que la página lee de llama-server. Sin parámetros por defecto: así no manda los de llama.cpp y el servidor
/// usa los de la receta.
fn props(t: &Target) -> Value {
    let ui = serde_json::from_slice::<Value>(BUILD_JSON).ok().and_then(|v| v["version"].as_str().map(str::to_string)).unwrap_or_default();
    json!({
        "default_generation_settings": { "params": {}, "n_ctx": t.ctx },
        "total_slots": 1,
        "model_alias": t.model,
        "model_path": t.model,
        "modalities": { "vision": false, "video": false, "audio": false },
        "endpoint_slots": false,
        "endpoint_props": false,
        "endpoint_metrics": false,
        "ui": true,
        "ui_settings": {},
        "chat_template": "",
        "chat_template_caps": { "supports_system_role": true, "supports_string_content": true, "supports_tools": true, "supports_tool_calls": true },
        "build_info": format!("{} (llama.cpp chat page {ui})", t.engine),
        "is_sleeping": false,
        "cors_proxy_enabled": false,
    })
}

/// La petición tal como la entiende un servidor compatible con OpenAI.
fn for_upstream(body: Map<String, Value>, model: &str) -> Map<String, Value> {
    let mut out: Map<String, Value> = body.into_iter().filter(|(k, _)| KEEP.contains(&k.as_str())).collect();
    // llama.cpp usa -1 para "sin límite"; vLLM lo rechaza. Sin el campo, cada servidor usa el suyo.
    for limit in ["max_tokens", "max_completion_tokens"] {
        if !out.get(limit).and_then(Value::as_u64).is_some_and(|n| n >= 1) {
            out.remove(limit);
        }
    }
    out.insert("model".into(), Value::String(model.into()));
    out
}

/// Respuesta (o trozo del stream) del servidor tal como la entiende la página. Devuelve si cambió algo.
fn for_page(v: &mut Value) -> bool {
    // Error con la forma de vLLM ({"object": "error", "message": ...}).
    if v.get("object").and_then(Value::as_str) == Some("error") {
        let message = v.get("message").cloned().unwrap_or_default();
        let code = v.get("code").cloned().unwrap_or_default();
        *v = json!({ "error": { "message": message, "type": "server_error", "code": code } });
        return true;
    }
    let mut changed = false;
    if let Some(choices) = v.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices {
            for part in ["delta", "message"] {
                if let Some(obj) = choice.get_mut(part).and_then(Value::as_object_mut) {
                    if !obj.contains_key("reasoning_content") {
                        if let Some(r) = obj.remove("reasoning") {
                            obj.insert("reasoning_content".into(), r);
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    changed
}

/// Una línea del stream (server-sent events), ya traducida.
fn rewrite_line(line: &[u8]) -> Bytes {
    let text = String::from_utf8_lossy(line);
    if let Some(data) = text.trim_end().strip_prefix("data:").map(str::trim) {
        if let Ok(mut v) = serde_json::from_str::<Value>(data) {
            if for_page(&mut v) {
                return Bytes::from(format!("data: {v}\n"));
            }
        }
    }
    Bytes::copy_from_slice(line)
}

async fn chat(req: Request<Incoming>, t: &Target, client: &reqwest::Client) -> Response<Body> {
    // Con imágenes o documentos adjuntos el mensaje puede pesar varios MB.
    let Ok(collected) = Limited::new(req.into_body(), 64 << 20).collect().await else {
        return error(StatusCode::PAYLOAD_TOO_LARGE, "invalid_request_error", "The message is too large.");
    };
    let Ok(Value::Object(body)) = serde_json::from_slice::<Value>(&collected.to_bytes()) else {
        return error(StatusCode::BAD_REQUEST, "invalid_request_error", "The request is not valid JSON.");
    };
    let body = for_upstream(body, &t.model);
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let sent = client.post(format!("{}/v1/chat/completions", t.upstream)).bearer_auth(&t.key).json(&body).send().await;
    let upstream = match sent {
        Ok(r) => r,
        Err(e) => return error(StatusCode::BAD_GATEWAY, "server_error", &format!("Could not reach the model server: {e}")),
    };
    let status = StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if !status.is_success() || !stream {
        let bytes = upstream.bytes().await.unwrap_or_default();
        let mut v = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| {
            let text = String::from_utf8_lossy(&bytes).trim().chars().take(300).collect::<String>();
            json!({ "object": "error", "message": if text.is_empty() { format!("The model server answered {status}.") } else { text } })
        });
        if !status.is_success() && v.get("error").is_none() && v.get("object").and_then(Value::as_str) != Some("error") {
            // Otras formas ({"detail": ...}): el texto entero como mensaje.
            let message = v.get("detail").or_else(|| v.get("message")).map_or_else(|| v.to_string(), |d| d.as_str().map_or_else(|| d.to_string(), str::to_string));
            v = json!({ "object": "error", "message": message, "code": status.as_u16() });
        }
        for_page(&mut v);
        return json_response(status, &v);
    }

    // El stream, línea a línea. Si la página corta (Stop, cerrar la ventana), se suelta la conexión con el servidor
    // y este deja de generar.
    let lines = futures_util::stream::unfold((upstream.bytes_stream(), Vec::<u8>::new(), false), |(mut up, mut pending, done)| async move {
        if done {
            return None;
        }
        loop {
            if let Some(end) = pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = pending.drain(..=end).collect();
                return Some((rewrite_line(&line), (up, pending, false)));
            }
            match up.next().await {
                Some(Ok(bytes)) => pending.extend_from_slice(&bytes),
                Some(Err(e)) => {
                    let lost = json!({ "error": { "message": format!("The connection to the model server was lost: {e}"), "type": "server_error" } });
                    return Some((Bytes::from(format!("data: {lost}\n\n")), (up, pending, true)));
                }
                None if pending.is_empty() => return None,
                None => {
                    let rest = std::mem::take(&mut pending);
                    return Some((rewrite_line(&rest), (up, pending, true)));
                }
            }
        }
    });
    let mut res = Response::new(BodyExt::boxed_unsync(StreamBody::new(lines.map(|b| Ok::<_, Infallible>(Frame::data(b))))));
    let headers = res.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llama_cpp_only_fields_are_left_out() {
        let body = json!({
            "messages": [{ "role": "user", "content": "hi" }],
            "stream": true,
            "model": "whatever-the-page-thinks",
            "max_tokens": -1,
            "reasoning_format": "auto",
            "reasoning_control": true,
            "return_progress": true,
            "timings_per_token": true,
            "backend_sampling": false,
            "samplers": ["top_k", "temperature"],
            "sse_ping_interval": 15,
            "tools": [{ "type": "function", "function": { "name": "get_datetime" } }],
            "temperature": 0.7,
        });
        let Value::Object(map) = body else { unreachable!() };
        let out = for_upstream(map, "qwen38-flash-next-fp4");
        let mut keys: Vec<&str> = out.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["messages", "model", "stream", "temperature", "tools"]);
        assert_eq!(out["model"], "qwen38-flash-next-fp4");

        let Value::Object(map) = json!({ "messages": [], "max_tokens": 512, "max_completion_tokens": 0 }) else { unreachable!() };
        let out = for_upstream(map, "m");
        assert_eq!(out["max_tokens"], 512);
        assert!(!out.contains_key("max_completion_tokens"));
    }

    #[test]
    fn vllm_reasoning_reaches_the_page_as_llama_cpp_sends_it() {
        let line = br#"data: {"choices":[{"index":0,"delta":{"reasoning":"Let me think"},"finish_reason":null}]}"#;
        let out = String::from_utf8(rewrite_line(&[line.as_slice(), b"\n"].concat()).to_vec()).unwrap();
        let v: Value = serde_json::from_str(out.trim().strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(v["choices"][0]["delta"]["reasoning_content"], "Let me think");
        assert!(v["choices"][0]["delta"].get("reasoning").is_none());
        assert!(out.ends_with('\n'));

        // Ya en la forma de llama.cpp, [DONE], líneas vacías y comentarios: tal cual.
        for same in [
            &br#"data: {"choices":[{"delta":{"reasoning_content":"x","content":"y"}}]}"#[..],
            b"data: [DONE]\n",
            b"\n",
            b": ping\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"ñ\"}}]}\n".as_bytes(),
        ] {
            assert_eq!(&rewrite_line(same)[..], same);
        }

        let mut whole = json!({ "choices": [{ "message": { "role": "assistant", "content": "Title", "reasoning": "r" } }] });
        assert!(for_page(&mut whole));
        assert_eq!(whole["choices"][0]["message"]["reasoning_content"], "r");
    }

    #[test]
    fn vllm_errors_take_the_shape_the_page_shows() {
        let mut v = json!({ "object": "error", "message": "max_tokens must be at least 1, got -1.", "type": "BadRequestError", "code": 400 });
        assert!(for_page(&mut v));
        assert_eq!(v["error"]["message"], "max_tokens must be at least 1, got -1.");
        assert_eq!(v["error"]["code"], 400);
    }

    #[test]
    fn props_describe_the_model_without_llama_cpp_defaults() {
        let t = Target { upstream: "http://127.0.0.1:8080".into(), key: "k".into(), model: "deepseek-v41-flash-1017".into(), ctx: 1_048_576, engine: "TensorFold" };
        let p = props(&t);
        assert_eq!(p["model_alias"], "deepseek-v41-flash-1017");
        assert_eq!(p["default_generation_settings"]["n_ctx"], 1_048_576);
        assert_eq!(p["default_generation_settings"]["params"], json!({}));
        assert_eq!(p["build_info"], "TensorFold (llama.cpp chat page b11827)");
        assert_eq!(models(&t)["data"][0]["id"], "deepseek-v41-flash-1017");
    }

    /// Un servidor tipo vLLM de una sola respuesta: guarda lo que le llega y contesta en stream.
    async fn fake_vllm() -> (String, tokio::sync::oneshot::Receiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = s.read(&mut buf).await.unwrap();
                req.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&req);
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let len = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length: ").map(|v| v.trim().parse::<usize>().unwrap())).unwrap_or(0);
                    if body.len() >= len {
                        break;
                    }
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&req).to_string());
            let events = [
                r#"data: {"choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}"#,
                r#"data: {"choices":[{"index":0,"delta":{"reasoning":"Thinking about ñ"}}]}"#,
                r#"data: {"choices":[{"index":0,"delta":{"content":"Hola"},"finish_reason":"stop"}]}"#,
                "data: [DONE]",
            ];
            s.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n").await.unwrap();
            for e in events {
                s.write_all(format!("{e}\n\n").as_bytes()).await.unwrap();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        (url, rx)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_chat_page_talks_to_vllm_through_the_proxy() {
        let (upstream, seen) = fake_vllm().await;
        let target = Target { upstream, key: "sk-local-test".into(), model: "qwen38-flash-next-fp4".into(), ctx: 262_144, engine: "vLLM" };
        let url = start(target.clone()).await.unwrap();
        assert_eq!(start(target).await.unwrap(), url, "the same server reuses the proxy, so the chat keeps its history");
        let http = reqwest::Client::new();

        // La página y lo público, sin clave; lo demás, con clave, como llama-server.
        let page = http.get(format!("{url}/")).send().await.unwrap();
        assert_eq!(page.status(), 200);
        assert!(page.text().await.unwrap().contains("_app/immutable/bundle."));
        let js = http.get(format!("{url}/_app/immutable/bundle.BUuvfZCd.js")).header("accept-encoding", "gzip").send().await.unwrap();
        assert_eq!(js.headers()["content-encoding"], "gzip");
        assert_eq!(http.get(format!("{url}/props")).send().await.unwrap().status(), 401);
        let props: Value = http.get(format!("{url}/props")).bearer_auth("sk-local-test").send().await.unwrap().json().await.unwrap();
        assert_eq!(props["model_alias"], "qwen38-flash-next-fp4");
        assert_eq!(http.post(format!("{url}/v1/chat/completions")).header("x-api-key", "wrong").send().await.unwrap().status(), 401);

        let body = json!({ "messages": [{ "role": "user", "content": "hola" }], "stream": true, "max_tokens": -1, "reasoning_format": "auto", "samplers": ["top_k"] });
        let answer = http.post(format!("{url}/v1/chat/completions")).bearer_auth("sk-local-test").json(&body).send().await.unwrap();
        assert_eq!(answer.status(), 200);
        assert_eq!(answer.headers()["content-type"], "text/event-stream");
        let text = answer.text().await.unwrap();
        assert!(text.contains(r#""reasoning_content":"Thinking about ñ""#), "{text}");
        assert!(text.contains(r#""content":"Hola""#) && text.contains("data: [DONE]"), "{text}");

        let request = seen.await.unwrap();
        assert!(request.starts_with("POST /v1/chat/completions "), "{request}");
        assert!(request.to_ascii_lowercase().contains("authorization: bearer sk-local-test"), "{request}");
        let sent: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(sent, json!({ "messages": [{ "role": "user", "content": "hola" }], "stream": true, "model": "qwen38-flash-next-fp4" }));

        // Parado: deja de contestar.
        stop();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(http.get(format!("{url}/health")).timeout(Duration::from_secs(2)).send().await.is_err());
    }

    #[test]
    fn the_vendored_page_is_complete() {
        let index = std::str::from_utf8(INDEX).unwrap();
        assert!(index.contains("_app/immutable/bundle.") && index.contains("_app/immutable/assets/bundle."));
        for gz in [BUNDLE_JS, BUNDLE_CSS] {
            let mut plain = Vec::new();
            flate2::read::GzDecoder::new(gz).read_to_end(&mut plain).unwrap();
            assert!(plain.len() > gz.len());
        }
        // La página es la del motor que usa la app: al cambiar de versión, scripts/vendor-llama-ui.sh.
        let version = &crate::catalog::get().unwrap().engine.version;
        assert_eq!(serde_json::from_slice::<Value>(BUILD_JSON).unwrap()["version"], *version, "run scripts/vendor-llama-ui.sh with llama.cpp {version}");
    }
}
