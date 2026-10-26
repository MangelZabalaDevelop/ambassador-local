//! GB10 ×2 con vLLM o TensorFold: verificador de que las dos GB10 pueden correr una receta.
//! No monta el cluster (eso lo hace NVIDIA Sync) ni cambia nada: solo mira, en esta GB10
//! y por SSH en la otra, lo mismo que necesitará el arranque.

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
        }
    }
}

/// Script de solo lectura; imprime `clave=valor`. Los valores de la receta ya vienen validados
/// (solo letras, números y `._/-@:`), así que se pueden poner tal cual.
fn probe_script(v: &Target) -> String {
    let prepared = match &v.prepared {
        Some((dir, stamp)) => format!("[ \"$(cat \"$HOME/{dir}/.ready\" 2>/dev/null)\" = \"{stamp}\" ] && echo prepared=ok || echo prepared=no\n"),
        None => String::new(),
    };
    format!(
        r#"docker info >/dev/null 2>&1 && echo docker=ok || echo docker=no
docker info --format '{{{{range $k, $v := .Runtimes}}}}{{{{$k}}}} {{{{end}}}}' 2>/dev/null | grep -qw nvidia && echo gpu_runtime=ok || echo gpu_runtime=no
{image_test} >/dev/null 2>&1 && echo image=ok || echo image=no
{prepared}S="${{HF_HOME:-$HOME/.cache/huggingface}}/hub/{snap}"
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

pub fn verify(v: &Target) -> Vec<Check> {
    if !cfg!(target_os = "linux") {
        return vec![check("GB10 with DGX OS", false, format!("{} models run only on two linked GB10.", v.engine))];
    }
    let pending = |label: &str| Check { label: label.into(), state: State::Wait, detail: "Needs the step above.".into() };
    let image_label = format!("{} engine image", v.engine);
    let mut labels = vec!["Docker with GPU access", image_label.as_str(), "Model weights"];
    if v.prepared.is_some() {
        labels.push("Prepared with prepare.sh");
    }
    labels.push("GPUs free");

    let Some((peer, ssh_ok)) = cluster::peer() else {
        let mut out = vec![check(
            "Second GB10 on the direct cable",
            false,
            "Not found. Connect the two GB10 with the ConnectX cable and run the cluster setup in NVIDIA Sync.",
        )];
        out.push(pending("Passwordless SSH to the second GB10"));
        out.extend(labels.iter().map(|l| pending(l)));
        return out;
    };
    let mut out = vec![check("Second GB10 on the direct cable", true, format!("Found at {peer}"))];
    out.push(check(
        "Passwordless SSH to the second GB10",
        ssh_ok,
        if ssh_ok { "Works over the direct cable".to_string() } else { "Refused. The cluster setup in NVIDIA Sync turns it on.".to_string() },
    ));
    if !ssh_ok {
        out.extend(labels.iter().map(|l| pending(l)));
        return out;
    }

    let script = probe_script(v);
    let (here, there) = std::thread::scope(|s| {
        let remote = s.spawn(|| run_remote(peer, &script));
        (run_local(&script), remote.join().unwrap_or_default())
    });
    out.push(both(labels[0], docker_state(&here), docker_state(&there), |s| s.to_string()));
    out.push(both(labels[1], get(&here, "image"), get(&there, "image"), |_| format!("missing {}", v.image_hint)));
    out.push(both(labels[2], get(&here, "weights"), get(&there, "weights"), |w| match w {
        "none" => format!("not downloaded ({} @ {})", v.repo, &v.revision[..7]),
        w if w.starts_with("missing") => format!("incomplete ({} files missing)", w.trim_start_matches("missing ")),
        w => w.to_string(),
    }));
    if v.prepared.is_some() {
        out.push(both("Prepared with prepare.sh", get(&here, "prepared"), get(&there, "prepared"), |_| "not yet. Run prepare.sh from the recipe folder.".into()));
    }
    out.push(both("GPUs free", get(&here, "gpu_free"), get(&there, "gpu_free"), |apps| format!("in use by {apps}. Stop it first.")));
    out
}

/// Qué revisar para la receta de este equipo, si corre en contenedor.
pub fn target_for(model: &crate::catalog::Model, hardware_id: &str) -> Option<Target> {
    model.vllm_for(hardware_id).map(Target::from).or_else(|| model.tensorfold_for(hardware_id).map(Target::from))
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

    /// Imprime el script de una receta para probarlo a mano en una GB10:
    /// `cargo test print_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_probe() {
        for m in &crate::catalog::get().unwrap().models {
            if let Some(t) = target_for(m, "pro-max-gb10-x2") {
                println!("#### {}
{}", t.repo, probe_script(&t));
            }
        }
    }
}
