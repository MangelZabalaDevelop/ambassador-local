// Espejo de los tipos de src-tauri/src/catalog.rs y lib.rs.

export type RemoteFile = { url: string; sha256: string; size: number };

export type Hardware = {
  id: string;
  name: string;
  kind: "tower" | "laptop" | "mini" | string;
  gpu: string;
  memory_gb: number;
  os: string[];
  arch: string[];
  gpu_match: string[];
  /** false = "Coming soon": todavía sin probar en ese equipo. */
  available: boolean;
  /** Equipos enlazados; 2 = dos GB10. */
  units: number;
  /** Foto en public/hardware/<image>.webp si no es la del id. */
  image?: string | null;
  specs: Specs;
};

export type Spec = { label: string; value: string };

export type Specs = {
  model: string;
  summary: string;
  highlights: Spec[];
  details: Spec[];
  best_for: string[];
};

export type Family = { id: string; name: string; series: string; description: string; color: string; featured?: boolean };

export type SizeClass = "small" | "medium" | "large" | "xlarge";

export type Recipe = {
  speed: number;
  /** Solo en estos sistemas; vacío = todos los del equipo (las de contenedor, solo Linux). */
  os?: ("windows" | "linux")[] | null;
  tokens_per_sec?: number | null;
  ctx_size: number;
  /** Otra cuantización para este equipo; va junto con `files`. */
  quant?: string | null;
  files?: RemoteFile[] | null;
  /** Lector de imágenes propio, cuando `files` viene de otro repositorio. */
  mmproj?: RemoteFile | null;
  /** Decodificación especulativa: MTP (la cabeza del modelo) o DFlash; `draft` es el GGUF aparte del que propone, si lo hay. */
  mtp?: { draft_n_max: number; method?: "mtp" | "dflash" | null; draft?: RemoteFile | null } | null;
  /** GB10 ×2: vLLM en contenedor en vez de llama.cpp. */
  vllm?: Vllm | null;
  /** GB10 ×2: TensorFold en contenedor, con imagen construida desde recipes/tensorfold/<recipe>/. */
  tensorfold?: TensorFold | null;
  /** Afinada a mano por el autor del catálogo: etiqueta "Miguel Tuned". */
  tuned?: boolean;
  /** Recomendada en este equipo: primera en la tabla, con coronita. */
  recommended?: boolean;
  gpu_layers?: number;
  flash_attn?: boolean;
  mmap?: boolean;
};

/** Imagen fijada por digest y pesos de Hugging Face fijados por commit; `size` en bytes por GB10. */
export type Vllm = { image: string; image_id: string; repo: string; revision: string; size: number; recipe?: string; engine?: "vLLM" | "llama.cpp"; include?: string[]; vision?: boolean };

/** Imagen local con el commit del motor en su etiqueta; pesos fijados por commit; `size` en bytes por GB10. */
export type TensorFold = { image: string; engine_commit: string; recipe: string; repo: string; revision: string; size: number };

/** Un punto del verificador del cluster: `wait` = no se pudo mirar porque falló uno anterior. */
export type ClusterCheck = { label: string; state: "ok" | "fail" | "wait"; detail: string };

export type Model = {
  id: string;
  family: string;
  name: string;
  size: string;
  tagline: string;
  quant: string;
  size_class: SizeClass;
  intelligence: number;
  /** Artificial Analysis Intelligence Index v5; null = not rated yet, so `intelligence` is an estimate. */
  aa_index?: number | null;
  best_for: string[];
  files: RemoteFile[];
  /** Lector de imágenes (--mmproj) para llama.cpp; vacío = solo texto. */
  mmproj?: RemoteFile | null;
  recipes: Record<string, Recipe>;
};

/** Archivos y cuantización que usa un equipo: los de su receta si los cambia, si no los del modelo. */
export const filesFor = (m: Model, hardwareId: string) => m.recipes[hardwareId]?.files ?? m.files;
/** Lector de imágenes de la receta con llama.cpp: el suyo si lo cambia, si no el del modelo. */
export const mmprojFor = (m: Model, hardwareId: string) => {
  const r = m.recipes[hardwareId];
  return !r || r.vllm || r.tensorfold ? null : (r.mmproj ?? m.mmproj ?? null);
};
/** Todo lo que baja para un equipo: el modelo y, si aplica, su archivo MTP y su lector de imágenes. */
export const downloadsFor = (m: Model, hardwareId: string) =>
  [...filesFor(m, hardwareId), m.recipes[hardwareId]?.mtp?.draft, mmprojFor(m, hardwareId)].filter((f): f is RemoteFile => !!f);
/** La receta de este equipo acepta imágenes. */
export const readsImages = (m: Model, hardwareId: string) => {
  const r = m.recipes[hardwareId];
  return r?.vllm ? !!r.vllm.vision : !!mmprojFor(m, hardwareId);
};
/** Bytes de disco que necesitan los pesos de la receta de este equipo (en contenedor: por GB10). */
export const diskFor = (m: Model, hardwareId: string) => {
  const r = m.recipes[hardwareId];
  const container = r?.vllm ?? r?.tensorfold;
  return container ? container.size : downloadsFor(m, hardwareId).reduce((a, f) => a + f.size, 0);
};
export const quantFor = (m: Model, hardwareId: string) => m.recipes[hardwareId]?.quant ?? m.quant;
/** Clave de `downloaded_models`: el mismo modelo puede estar bajado para un equipo y no para otro. */
export const downloadKey = (m: Model, hardwareId: string) => `${m.id}@${hardwareId}`;

export type Catalog = {
  schema: number;
  engine: { name: string; version: string };
  hardware: Hardware[];
  families: Family[];
  models: Model[];
};

export type System = {
  os: string;
  arch: string;
  gpus: { name: string; memory_mib: number | null }[];
  suggested_hardware: string | null;
  /** Equipos del catálogo que coinciden con esta máquina; solo esos se pueden usar. */
  detected_hardware: string[];
};

/** Versión nueva publicada en las releases del repo, firmada con nuestra clave. */
export type UpdateInfo = { version: string; current: string; notes: string | null };

/** Qué modelo y en qué equipo: viene en cada estado activo (descarga, arranque, en marcha). */
export type Job = { hardware_id: string; model_id: string };

export type Status =
  | { stage: "idle" }
  | ({ stage: "downloading"; label: string; step: number; steps: number; downloaded: number; total: number } & Job)
  /** label: what the engine step is doing, when it is not unpacking llama.cpp (a container recipe pulling or building its image). */
  | ({ stage: "extracting"; label?: string } & Job)
  | ({ stage: "starting" } & Job)
  | ({ stage: "ready"; url: string; api_url: string; api_key: string; lan_url: string | null } & Job)
  /** Con el modelo cuando el error es de uno concreto (no arrancó, o se cayó estando en marcha). */
  | ({ stage: "error"; message: string } & Partial<Job>);

/** El modelo que el servidor descarga, arranca o sirve; null si no hay ninguno (parado o con error). */
export const jobOf = (s: Status): Job | null =>
  s.stage === "idle" || s.stage === "error" ? null : { hardware_id: s.hardware_id, model_id: s.model_id };

/** Contexto en tokens, corto: 131072 → "128K", 1048576 → "1M". */
export const formatContext = (tokens: number) =>
  tokens >= 1048576 ? `${+(tokens / 1048576).toFixed(1)}M` : `${(tokens / 1024).toFixed(0)}K`;

/** Cómo se quita esta copia: "windows" (su instalador), "deb" (apt) o "appimage"; null = no se instaló (`reason` dice por qué). */
export type UninstallInfo = {
  method: "windows" | "deb" | "appimage" | null;
  reason: string | null;
  package: string | null;
  /** Quitar el .deb pide la contraseña de administrador (no si la app ya corre como root). */
  asks_password: boolean;
  data_bytes: number;
};
/** Por qué no se pudo, y el comando para terminarlo a mano si hace falta (p. ej. sin ventana de contraseña). */
export type UninstallFailure = { message: string; command: string | null };
/** `closing`: todo bien, la app se cierra sola. Si no, `leftovers` no se pudieron borrar (`command` los borra). */
export type UninstallDone = { closing: boolean; leftovers: string[]; command: string | null };
