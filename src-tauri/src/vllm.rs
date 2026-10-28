//! vLLM o TensorFold: verificador de que el equipo puede correr una receta en contenedor.
//! En GB10 ×2 no monta el cluster (eso lo hace NVIDIA Sync) ni cambia nada: solo mira, en esta GB10
//! y por SSH en la otra, lo mismo que necesitará el arranque. En un equipo solo (T2) mira solo aquí.

use crate::catalog::{TensorFold, Vllm};
use crate::cluster;
use serde::Serialize;
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Ok,
    Fail,
    /// No se pudo mirar porque falló un paso anterior.
    Wait,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub label: String,
    pub state: State,
    pub detail: String,
}

fn check(label: &str, ok: bool, detail: impl Into<String>) -> Check {
    Check { label: label.into(), state: if ok { State::Ok } else { State::Fail }, detail: detail.into() }
}

/// Lo que el verificador mira de una receta en contenedor, sea de vLLM o de TensorFold.
pub struct Target {
    engine: &'static str,
    /// Comando de shell que sale con 0 si la imagen está.
    image_test: String,
    image_hint: String,
    snapshot: String,
    repo: String,
    revision: String,
    /// TensorFold: carpeta de `prepare.sh` (relativa a $HOME) y lo que debe decir su `.ready`.
    prepared: Option<(String, String)>,
    /// Un equipo solo (T2): sin cable directo ni segunda GB10.
    single: bool,
}

impl From<&Vllm> for Target {
    fn from(v: &Vllm) -> Self {
        Target {
            engine: "vLLM",
            image_test: format!("{{ docker image inspect {} || docker image inspect {}; }}", v.image, v.image_id),
            image_hint: short_image(&v.image),
            snapshot: v.snapshot_dir(),
            repo: v.repo.clone(),
            revision: v.revision.clone(),
            prepared: None,
            single: false,
        }
    }
}

impl From<&TensorFold> for Target {
    fn from(t: &TensorFold) -> Self {
        Target {
            engine: "TensorFold",
            // La imagen se construye en casa: vale si su etiqueta lleva el commit del motor de la receta.
            image_test: format!(
                "[ \"$(docker image inspect -f '{{{{index .Config.Labels \"io.github.ambassadorlocal.engine\"}}}}' {} 2>/dev/null)\" = \"bertholomus/TensorFold@{}\" ]",
                t.image, t.engine_commit
            ),
            image_hint: format!("{} (build it with prepare.sh)", t.image),
            snapshot: t.snapshot_dir(),
            repo: t.repo.clone(),
            revision: t.revision.clone(),
            prepared: Some((format!(".cache/ambassador-local/tensorfold/{}", t.recipe), format!("{} {}", t.engine_commit, t.revision))),
            single: false,
        }
    }
}

/// Script de solo lectura; imprime `clave=valor`. Los valores de la receta ya vienen validados
/// (solo letras, números y `._/-@:`), así que se pueden poner tal cual.
fn probe_script(v: &Target) -> String {
    // TensorFold: las rutas que el embajador movió a mano (p. ej. HF_HUB_CACHE en otro disco) están en local.env,
    // que config.sh lee igual antes de arrancar.
    let (local_env, prepared) = match &v.prepared {
        Some((dir, stamp)) => (
            format!("[ -f \"$HOME/{dir}/local.env\" ] && {{ set -a; . \"$HOME/{dir}/local.env\"; set +a; }}\n"),
            format!("[ \"$(cat \"$HOME/{dir}/.ready\" 2>/dev/null)\" = \"{stamp}\" ] && echo prepared=ok || echo prepared=no\n"),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        r#"{local_env}docker info >/dev/null 2>&1 && echo docker=ok || echo docker=no
docker info --format '{{{{range $k, $v := .Runtimes}}}}{{{{$k}}}} {{{{end}}}}' 2>/dev/null | grep -qw nvidia && echo gpu_runtime=ok || echo gpu_runtime=no
{image_test} >/dev/null 2>&1 && echo image=ok || echo image=no
{prepared}S="${{HF_HUB_CACHE:-${{HF_HOME:-$HOME/.cache/huggingface}}/hub}}/{snap}"
if [ ! -d "$S" ]; then echo weights=none
else
  n=$(find -L "$S" -type l 2>/dev/null | wc -l)
  for f in $(cat "$S"/*.safetensors.index.json 2>/dev/null | grep -o '"[^"]*\.safetensors"' | tr -d '"' | sort -u); do [ -e "$S/$f" ] || n=$((n+1)); done
  find -L "$S" -name "*.safetensors" | grep -q . || n=$((n+1))
  [ "$n" = 0 ] && echo weights=ok || echo "weights=missing $n"
fi
apps=$(nvidia-smi --query-compute-apps=name --format=csv,noheader 2>/dev/null | sed 's/.*\///' | sort -u | paste -sd, -)
[ -z "$apps" ] && echo gpu_free=ok || echo "gpu_free=$apps"
"#,
        image_test = v.image_test,
        snap = v.snapshot,
    )
}

fn parse(out: &str) -> HashMap<String, String> {
    out.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

fn run_local(script: &str) -> HashMap<String, String> {
    let out = Command::new("bash").args(["-c", script]).stdin(Stdio::null()).output();
    out.map(|o| parse(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

fn ssh_cmd(peer: Ipv4Addr) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-n", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", &peer.to_string()]);
    c
}

fn run_remote(peer: Ipv4Addr, script: &str) -> HashMap<String, String> {
    let out = ssh_cmd(peer).arg(script).stdin(Stdio::null()).output();
    out.map(|o| parse(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

fn get(m: &HashMap<String, String>, key: &str) -> String {
    m.get(key).cloned().unwrap_or_else(|| "no answer".into())
}

fn docker_state(m: &HashMap<String, String>) -> String {
    match (get(m, "docker").as_str(), get(m, "gpu_runtime").as_str()) {
        ("ok", "ok") => "ok".into(),
        ("ok", _) => "no NVIDIA container runtime".into(),
        ("no", _) => "Docker not reachable without sudo".into(),
        _ => "no answer".into(),
    }
}

/// Une el resultado de las dos GB10 en una línea: ✓ si las dos están bien.
fn both(label: &str, a: String, b: String, explain: impl Fn(&str) -> String) -> Check {
    if a == "ok" && b == "ok" {
        return check(label, true, "Both GB10");
    }
    let side = |name: &str, v: &str| if v == "ok" { format!("{name}: ok") } else { format!("{name}: {}", explain(v)) };
    check(label, false, format!("{} · {}", side("This GB10", &a), side("Second GB10", &b)))
}

/// Un solo equipo: ✓ con "Ready", o lo que falta.
fn one(label: &str, a: String, explain: impl Fn(&str) -> String) -> Check {
    if a == "ok" {
        check(label, true, "Ready")
    } else {
        check(label, false, explain(&a))
    }
}

pub fn verify(v: &Target) -> Vec<Check> {
    if !cfg!(target_os = "linux") {
        let (label, where_) = if v.single { ("Linux", "on Linux") } else { ("GB10 with DGX OS", "only on two linked GB10") };
        return vec![check(label, false, format!("{} models run {where_}.", v.engine))];
    }
    let pending = |label: &str| Check { label: label.into(), state: State::Wait, detail: "Needs the step above.".into() };
    let image_label = "Engine image".to_string();
    let mut labels = vec!["Docker with GPU access", image_label.as_str(), "Model weights"];
    if v.prepared.is_some() {
        labels.push("Prepared with prepare.sh");
    }
    labels.push(if v.single { "GPU free" } else { "GPUs free" });

    let mut out = Vec::new();
    let script = probe_script(v);
    let (here, there) = if v.single {
        (run_local(&script), None)
    } else {
        let Some((peer, ssh_ok)) = cluster::peer() else {
            out.push(check(
                "Second GB10 on the direct cable",
                false,
                "Not found. Connect the two GB10 with the ConnectX cable and run the cluster setup in NVIDIA Sync.",
            ));
            out.push(pending("Passwordless SSH to the second GB10"));
            out.extend(labels.iter().map(|l| pending(l)));
            return out;
        };
        out.push(check("Second GB10 on the direct cable", true, format!("Found at {peer}")));
        out.push(check(
            "Passwordless SSH to the second GB10",
            ssh_ok,
            if ssh_ok { "Works over the direct cable".to_string() } else { "Refused. The cluster setup in NVIDIA Sync turns it on.".to_string() },
        ));
        if !ssh_ok {
            out.extend(labels.iter().map(|l| pending(l)));
            return out;
        }
        std::thread::scope(|s| {
            let remote = s.spawn(|| run_remote(peer, &script));
            (run_local(&script), Some(remote.join().unwrap_or_default()))
        })
    };
    // Una fila: en GB10 ×2 junta las dos respuestas, en un equipo solo usa la de aquí.
    let row = |label: &str, value: &dyn Fn(&HashMap<String, String>) -> String, explain: &dyn Fn(&str) -> String| match &there {
        Some(t) => both(label, value(&here), value(t), explain),
        None => one(label, value(&here), explain),
    };
    out.push(row(labels[0], &docker_state, &|s| s.to_string()));
    out.push(row(labels[1], &|m| get(m, "image"), &|_| format!("missing {}", v.image_hint)));
    out.push(row(labels[2], &|m| get(m, "weights"), &|w| match w {
        "none" => format!("not downloaded ({} @ {})", v.repo, &v.revision[..7]),
        w if w.starts_with("missing") => format!("incomplete ({} files missing)", w.trim_start_matches("missing ")),
        w => w.to_string(),
    }));
    if v.prepared.is_some() {
        out.push(row("Prepared with prepare.sh", &|m| get(m, "prepared"), &|_| "not yet. Run prepare.sh from the recipe folder.".into()));
    }
    out.push(row(labels[labels.len() - 1], &|m| get(m, "gpu_free"), &|apps| format!("in use by {apps}. Stop it first.")));
    out
}

/// Qué revisar para la receta de este equipo, si corre en contenedor.
pub fn target_for(model: &crate::catalog::Model, hardware: &crate::catalog::Hardware) -> Option<Target> {
    let id = hardware.id.as_str();
    let mut t = model.vllm_for(id).map(Target::from).or_else(|| model.tensorfold_for(id).map(Target::from))?;
    t.single = hardware.units <= 1;
    Some(t)
}

/// `repo@sha256:abcdef…` → `repo@abcdef1` para mostrarlo corto.
fn short_image(image: &str) -> String {
    match image.split_once("@sha256:") {
        Some((name, digest)) => format!("{name}@{}", &digest[..7.min(digest.len())]),
        None => image.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_image_and_parse() {
        assert_eq!(short_image("eugr/x@sha256:0123456789"), "eugr/x@0123456");
        let m = parse("docker=ok\nweights=missing 2\nnoise\n");
        assert_eq!(get(&m, "weights"), "missing 2");
        assert_eq!(get(&m, "image"), "no answer");
    }

    /// El verificador solo corre en Linux (en Windows `verify` responde sin mirar nada) y necesita bash.
    #[cfg(target_os = "linux")]
    mod probe {
        use super::*;

        /// Corre el script de verdad con bash en un $HOME de prueba (sin docker ni GPU: solo mira los pesos).
        fn probe_in(home: &std::path::Path, v: &Target, env: &[(&str, &std::path::Path)]) -> HashMap<String, String> {
            let mut cmd = Command::new("bash");
            cmd.args(["-c", &probe_script(v)]).env("HOME", home).env_remove("HF_HOME").env_remove("HF_HUB_CACHE");
            for (k, val) in env {
                cmd.env(k, val);
            }
            parse(&String::from_utf8_lossy(&cmd.output().unwrap().stdout))
        }

        /// Una revisión con su índice y dos archivos de pesos, como la deja `hf download`.
        fn snapshot(hub: &std::path::Path, t: &Target, missing: bool) {
            let dir = hub.join(&t.snapshot);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("model.safetensors.index.json"), r#"{"weight_map": {"a": "model-1.safetensors", "b": "model-2.safetensors"}}"#).unwrap();
            std::fs::write(dir.join("model-1.safetensors"), "x").unwrap();
            if !missing {
                std::fs::write(dir.join("model-2.safetensors"), "x").unwrap();
            }
        }

        fn scratch(name: &str) -> std::path::PathBuf {
            let d = std::env::temp_dir().join(format!("al-probe-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            d
        }

        fn first_target(tensorfold: bool) -> Target {
            let cat = crate::catalog::get().unwrap();
            cat.models
                .iter()
                .flat_map(|m| cat.hardware.iter().filter_map(move |h| target_for(m, h)))
                .find(|t| t.prepared.is_some() == tensorfold)
                .unwrap()
        }

        #[test]
        fn probe_finds_weights_in_the_default_cache() {
            let home = scratch("default");
            let t = first_target(false);
            snapshot(&home.join(".cache/huggingface/hub"), &t, false);
            assert_eq!(get(&probe_in(&home, &t, &[]), "weights"), "ok");
        }

        #[test]
        fn probe_reports_missing_weight_files() {
            let home = scratch("missing");
            let t = first_target(false);
            snapshot(&home.join(".cache/huggingface/hub"), &t, true);
            assert_eq!(get(&probe_in(&home, &t, &[]), "weights"), "missing 1");
            assert_eq!(get(&probe_in(&scratch("none"), &t, &[]), "weights"), "none");
        }

        /// Igual que config.sh: HF_HUB_CACHE manda sobre HF_HOME y sobre la ruta por defecto.
        #[test]
        fn probe_follows_hf_hub_cache_like_the_recipe_scripts() {
            let home = scratch("hubcache");
            let hub = home.join("other-disk/hub");
            let t = first_target(false);
            snapshot(&hub, &t, false);
            assert_eq!(get(&probe_in(&home, &t, &[("HF_HUB_CACHE", &hub)]), "weights"), "ok");
        }

        /// TensorFold: lo que el embajador fijó en local.env (pesos en otro disco) y el sello de prepare.sh.
        #[test]
        fn probe_reads_tensorfold_local_env_and_ready_stamp() {
            let home = scratch("tensorfold");
            let t = first_target(true);
            let (dir, stamp) = t.prepared.clone().unwrap();
            let hub = home.join("big-disk/hub");
            snapshot(&hub, &t, false);
            std::fs::create_dir_all(home.join(&dir)).unwrap();
            std::fs::write(home.join(&dir).join("local.env"), format!("HF_HUB_CACHE={}\n", hub.display())).unwrap();
            let m = probe_in(&home, &t, &[]);
            assert_eq!(get(&m, "weights"), "ok");
            assert_eq!(get(&m, "prepared"), "no");
            std::fs::write(home.join(&dir).join(".ready"), format!("{stamp}\n")).unwrap();
            assert_eq!(get(&probe_in(&home, &t, &[]), "prepared"), "ok");
        }
    }

    /// Imprime el script de una receta para probarlo a mano en una GB10:
    /// `cargo test print_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_probe() {
        let cat = crate::catalog::get().unwrap();
        for m in &cat.models {
            if let Some(t) = cat.hardware.iter().find_map(|h| target_for(m, h)) {
                println!("#### {}
{}", t.repo, probe_script(&t));
            }
        }
    }
}
