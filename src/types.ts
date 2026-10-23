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
  tokens_per_sec?: number | null;
  ctx_size: number;
  /** Otra cuantización para este equipo; va junto con `files`. */
  quant?: string | null;
  files?: RemoteFile[] | null;
  gpu_layers?: number;
  flash_attn?: boolean;
  mmap?: boolean;
};

export type Model = {
  id: string;
  family: string;
  name: string;
  size: string;
  tagline: string;
  quant: string;
  size_class: SizeClass;
  intelligence: number;
  best_for: string[];
  files: RemoteFile[];
  recipes: Record<string, Recipe>;
};

/** Archivos y cuantización que usa un equipo: los de su receta si los cambia, si no los del modelo. */
export const filesFor = (m: Model, hardwareId: string) => m.recipes[hardwareId]?.files ?? m.files;
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
};

export type Status =
  | { stage: "idle" }
  | { stage: "downloading"; label: string; step: number; steps: number; downloaded: number; total: number }
  | { stage: "extracting" }
  | { stage: "starting"; model_id: string }
  | { stage: "ready"; model_id: string; url: string; api_url: string; api_key: string }
  | { stage: "error"; message: string };
