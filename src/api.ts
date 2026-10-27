import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Catalog, ClusterCheck, Status, System, UninstallDone, UninstallFailure, UninstallInfo, UpdateInfo } from "./types";
import bundled from "../recipes/catalog.json";

const inTauri = "__TAURI_INTERNALS__" in window;

const tauriApi = {
  catalog: () => invoke<Catalog>("get_catalog"),
  detect: () => invoke<System>("detect_system"),
  downloaded: () => invoke<string[]>("downloaded_models"),
  currentStatus: () => invoke<[Status, string[]]>("current_status"),
  checkCluster: (hardwareId: string, modelId: string) => invoke<ClusterCheck[]>("check_cluster", { hardwareId, modelId }),
  start: (hardwareId: string, modelId: string, lan: boolean) => invoke<void>("start_model", { hardwareId, modelId, lan }),
  stop: () => invoke<void>("stop_model"),
  cancel: () => invoke<void>("cancel"),
  version: () => getVersion(),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
  uninstallInfo: () => invoke<UninstallInfo>("uninstall_info"),
  // El backend responde los fallos como { message, command }; cualquier otro error, como texto.
  uninstall: (deleteData: boolean) => invoke<UninstallDone>("uninstall", { deleteData }).catch((e) => Promise.reject(asFailure(e))),
  quit: () => invoke<void>("quit"),
  onStatus: (cb: (s: Status) => void) => listen<Status>("status", (e) => cb(e.payload)),
  onLog: (cb: (line: string) => void) => listen<string>("log", (e) => cb(e.payload)),
};

// Solo para diseñar la interfaz con `npm run dev` en un navegador normal.
function mockApi(): typeof tauriApi {
  let emit: (s: Status) => void = () => {};
  // Como el backend: Cancel y Stop cortan la tarea en curso.
  let cancelled = false;
  // ?hw=pro-max-gb10-x2 simula otro equipo detectado, para ver su pantalla.
  const hw = new URLSearchParams(location.search).get("hw") ?? "pro-max-t2";
  const system: System = { os: "windows", arch: "x86_64", gpus: [{ name: "NVIDIA RTX PRO 6000 Blackwell", memory_mib: 98304 }], suggested_hardware: hw, detected_hardware: [hw] };
  return {
    // Mismos valores por defecto que pone Rust al leer el catálogo.
    catalog: async () => ({ ...bundled, hardware: bundled.hardware.map((h) => ({ available: true, units: 1, ...h })) }) as unknown as Catalog,
    detect: async () => system,
    downloaded: async () => [],
    currentStatus: async () => [{ stage: "idle" }, []],
    // Lo que respondió el verificador en las Spark reales mientras corría otra prueba.
    checkCluster: async (hardwareId, modelId) => {
      await new Promise((r) => setTimeout(r, 900));
      const tensorfold = !!(bundled.models.find((m) => m.id === modelId)?.recipes as Record<string, { tensorfold?: unknown }> | undefined)?.[hardwareId]?.tensorfold;
      return [
        { label: "Second GB10 on the direct cable", state: "ok", detail: "Found at 10.100.88.1" },
        { label: "Passwordless SSH to the second GB10", state: "ok", detail: "Works over the direct cable" },
        { label: "Docker with GPU access", state: "ok", detail: "Both GB10" },
        { label: `${tensorfold ? "TensorFold" : "vLLM"} engine image`, state: "ok", detail: "Both GB10" },
        { label: "Model weights", state: "ok", detail: "Both GB10" },
        ...(tensorfold ? [{ label: "Prepared with prepare.sh", state: "ok" as const, detail: "Both GB10" }] : []),
        { label: "GPUs free", state: "fail", detail: "This GB10: in use by ggml-rpc-server. Stop it first. · Second GB10: in use by llama-server. Stop it first." },
      ];
    },
    start: async (hardwareId, modelId, lan) => {
      const job = { hardware_id: hardwareId, model_id: modelId };
      const name = bundled.models.find((m) => m.id === modelId)?.name ?? modelId;
      cancelled = false;
      const stopped = () => {
        if (cancelled) emit({ stage: "idle" });
        return cancelled;
      };
      for (let i = 0; i <= 10; i++) {
        emit({ stage: "downloading", ...job, label: name, step: 1, steps: 1, downloaded: i * 7.3e8, total: 7.3e9 });
        await new Promise((r) => setTimeout(r, 200));
        if (stopped()) return;
      }
      emit({ stage: "starting", ...job });
      await new Promise((r) => setTimeout(r, 800));
      if (stopped()) return;
      emit({ stage: "ready", ...job, url: "http://127.0.0.1:8080", api_url: "http://127.0.0.1:8080/v1", api_key: "sk-local-preview-0000000000000000000000000000", lan_url: lan ? "http://192.168.1.20:8080/v1" : null });
    },
    stop: async () => {
      cancelled = true;
      emit({ stage: "idle" });
    },
    // Como el backend: cancelar vuelve a "listo para arrancar".
    cancel: async () => {
      cancelled = true;
    },
    version: async () => "0.1.0",
    checkUpdate: async () => ({ version: "0.2.0", current: "0.1.0", notes: "New models for GB10." }),
    installUpdate: async () => {
      await new Promise((r) => setTimeout(r, 1500));
      throw new Error("Preview only.");
    },
    uninstallInfo: async () => ({ method: "deb", reason: null, package: "ambassador-local", asks_password: true, data_bytes: 8.24e10 }),
    uninstall: async () => {
      await new Promise((r) => setTimeout(r, 1500));
      return Promise.reject<UninstallDone>({ message: "Preview only: nothing was uninstalled.", command: null });
    },
    quit: async () => {},
    onStatus: async (cb) => {
      emit = cb;
      return () => {};
    },
    onLog: async () => () => {},
  };
}

export const api = inTauri ? tauriApi : mockApi();

function asFailure(e: unknown): UninstallFailure {
  if (e && typeof e === "object" && "message" in e) return { message: String(e.message), command: "command" in e && e.command ? String(e.command) : null };
  return { message: String(e), command: null };
}

export function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(0)} MB`;
  return `${(n / 1e3).toFixed(0)} KB`;
}
