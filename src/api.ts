import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Catalog, Status, System, UpdateInfo } from "./types";
import bundled from "../recipes/catalog.json";

const inTauri = "__TAURI_INTERNALS__" in window;

const tauriApi = {
  catalog: () => invoke<Catalog>("get_catalog"),
  detect: () => invoke<System>("detect_system"),
  downloaded: () => invoke<string[]>("downloaded_models"),
  currentStatus: () => invoke<[Status, string[]]>("current_status"),
  start: (hardwareId: string, modelId: string) => invoke<void>("start_model", { hardwareId, modelId }),
  stop: () => invoke<void>("stop_model"),
  cancel: () => invoke<void>("cancel"),
  version: () => getVersion(),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  installUpdate: () => invoke<void>("install_update"),
  onStatus: (cb: (s: Status) => void) => listen<Status>("status", (e) => cb(e.payload)),
  onLog: (cb: (line: string) => void) => listen<string>("log", (e) => cb(e.payload)),
};

// Solo para diseñar la interfaz con `npm run dev` en un navegador normal.
function mockApi(): typeof tauriApi {
  let emit: (s: Status) => void = () => {};
  const system: System = { os: "windows", arch: "x86_64", gpus: [{ name: "NVIDIA RTX PRO 6000 Blackwell", memory_mib: 98304 }], suggested_hardware: "pro-max-t2" };
  return {
    catalog: async () => bundled as Catalog,
    detect: async () => system,
    downloaded: async () => [],
    currentStatus: async () => [{ stage: "idle" }, []],
    start: async (_h, modelId) => {
      for (let i = 0; i <= 10; i++) {
        emit({ stage: "downloading", label: "Gemma 3 12B", step: 1, steps: 1, downloaded: i * 7.3e8, total: 7.3e9 });
        await new Promise((r) => setTimeout(r, 200));
      }
      emit({ stage: "starting", model_id: modelId });
      await new Promise((r) => setTimeout(r, 800));
      emit({ stage: "ready", model_id: modelId, url: "http://127.0.0.1:8080", api_url: "http://127.0.0.1:8080/v1", api_key: "sk-local-preview-0000000000000000000000000000" });
    },
    stop: async () => emit({ stage: "idle" }),
    cancel: async () => emit({ stage: "error", message: "Cancelled." }),
    version: async () => "0.1.0",
    checkUpdate: async () => ({ version: "0.2.0", current: "0.1.0", notes: "New models for GB10." }),
    installUpdate: async () => {
      await new Promise((r) => setTimeout(r, 1500));
      throw new Error("Preview only.");
    },
    onStatus: async (cb) => {
      emit = cb;
      return () => {};
    },
    onLog: async () => () => {},
  };
}

export const api = inTauri ? tauriApi : mockApi();

export function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(0)} MB`;
  return `${(n / 1e3).toFixed(0)} KB`;
}
