import { useEffect, useRef, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { formatBytes } from "../api";
import { filesFor, quantFor, type Family, type Hardware, type Model, type Status } from "../types";
import { labLogoSrc } from "./Illustrations";

type Props = {
  hardware: Hardware;
  model: Model;
  lab?: Family;
  downloaded: boolean;
  status: Status;
  logs: string[];
  onRun: () => void;
  onCancel: () => void;
  onStop: () => void;
};

const STAGES = ["Download", "Install engine", "Load on GPU", "Live"];

/** Pantalla final: tarjeta "hero" con el estado del servidor y, cuando está en marcha, cómo usarlo. */
export function RunScreen(props: Props) {
  const { status, hardware, model, lab } = props;
  const recipe = model.recipes[hardware.id];
  const live = status.stage === "ready";
  const active = status.stage !== "idle" && status.stage !== "error";
  const size = filesFor(model, hardware.id).reduce((a, f) => a + f.size, 0);

  return (
    <div className="@container mt-8 flex flex-col gap-5">
      <section className={`hero relative overflow-hidden rounded-2xl text-white shadow-2xl ${live ? "hero-live" : ""}`}>
        <div className="hero-grid pointer-events-none absolute inset-0" />

        <div className="relative grid grid-cols-1 items-center gap-6 px-6 pt-7 pb-6 @2xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] @2xl:gap-8 @2xl:px-8 @2xl:pt-8 @2xl:pb-7">
          <div className="relative grid place-items-center">
            <div className={`absolute h-44 w-44 rounded-full blur-3xl transition-colors duration-700 ${live ? "bg-[#76b900]/40" : "bg-[#0672cb]/40"}`} />
            <img src={`/hardware/${hardware.id}.webp`} alt={hardware.name} className="relative max-h-32 max-w-full object-contain drop-shadow-2xl @2xl:max-h-44" draggable={false} />
          </div>

          <div className="min-w-0">
            <div className="flex items-center gap-3">
              {lab && labLogoSrc(lab.id) && (
                <span className="grid h-8 w-8 place-items-center rounded-lg bg-white shadow">
                  <img src={labLogoSrc(lab.id)} alt="" className="h-5 w-5" draggable={false} />
                </span>
              )}
              <span className="text-xs font-bold tracking-[0.2em] text-white/60 uppercase">
                {lab ? `${lab.name} · ${lab.series}` : "Model"}
              </span>
              <StatusPill status={status} />
            </div>
            <h2 className="mt-3 text-3xl font-bold tracking-tight @2xl:text-4xl">{model.name}</h2>
            <div className="mt-1 text-white/60">
              on {hardware.name} · {hardware.gpu}
            </div>
            <div className="mt-6 grid grid-cols-2 gap-2 @lg:grid-cols-4">
              <Tile label="Parameters" value={model.size} />
              <Tile label="Quantization" value={quantFor(model, hardware.id)} />
              <Tile label="Context" value={`${(recipe.ctx_size / 1024).toFixed(0)}K`} />
              <Tile label={props.downloaded ? "On disk" : "Download"} value={formatBytes(size)} />
            </div>
          </div>
        </div>

        <div className="relative border-t border-white/10 bg-black/25 px-6 py-5 backdrop-blur @2xl:px-8">
          <ActionBar {...props} />
        </div>
      </section>

      {active && <Stepper status={status} />}

      {status.stage === "ready" && <ReadyCards status={status} modelName={model.name} />}

      <ServerLog logs={props.logs} />
    </div>
  );
}

function StatusPill({ status }: { status: Status }) {
  const map: Record<Status["stage"], [string, string]> = {
    idle: ["Ready to run", "bg-white/10 text-white/70"],
    downloading: ["Downloading", "bg-[#2f8fff]/20 text-[#8cc2ff]"],
    extracting: ["Installing", "bg-amber-400/20 text-amber-200"],
    starting: ["Starting", "bg-amber-400/20 text-amber-200"],
    ready: ["Live", "bg-[#76b900]/20 text-[#b6f04a]"],
    error: ["Stopped", "bg-red-500/20 text-red-200"],
  };
  const [text, cls] = map[status.stage];
  return (
    <span className={`ml-auto flex items-center gap-2 rounded-full px-3 py-1 text-xs font-bold ${cls}`}>
      <span className={`h-2 w-2 rounded-full bg-current ${status.stage === "ready" ? "pulse-dot" : ""}`} />
      {text}
    </span>
  );
}

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-xl border border-white/10 bg-white/[0.06] px-3 py-2.5">
      <div className="truncate text-lg leading-tight font-bold">{value}</div>
      <div className="mt-0.5 text-[11px] text-white/55">{label}</div>
    </div>
  );
}

function ActionBar(props: Props) {
  const { status } = props;

  if (status.stage === "idle" || status.stage === "error") {
    return (
      <div className="flex flex-wrap items-center gap-x-6 gap-y-4">
        <div className="min-w-[16rem] flex-1">
          {status.stage === "error" ? (
            <div className="text-sm text-red-200">
              <span className="font-bold">Stopped: </span>
              {status.message}
            </div>
          ) : (
            <Assurances />
          )}
        </div>
        <button
          onClick={props.onRun}
          className="run-button flex items-center gap-3 rounded-xl px-9 py-3.5 text-lg font-bold text-white transition hover:-translate-y-0.5"
        >
          <Icon d="M7 4.5v15l13-7.5z" fill />
          {status.stage === "error" ? "Try again" : "Run"}
        </button>
      </div>
    );
  }

  if (status.stage === "downloading") return <DownloadBar status={status} onCancel={props.onCancel} />;

  if (status.stage === "extracting" || status.stage === "starting") {
    return (
      <div className="flex flex-wrap items-center gap-x-6 gap-y-4">
        <div className="min-w-[16rem] flex-1">
          <div className="flex items-center gap-3 font-bold">
            <span className="h-4 w-4 animate-spin rounded-full border-2 border-white/30 border-t-white" />
            {status.stage === "extracting" ? "Installing the llama.cpp engine…" : "Loading the model onto the GPU…"}
          </div>
          <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-white/10">
            <div className="indeterminate h-full w-1/3 rounded-full bg-gradient-to-r from-[#2f8fff] to-[#76b900]" />
          </div>
        </div>
        <GhostButton onClick={props.onCancel}>Cancel</GhostButton>
      </div>
    );
  }

  return <LiveBar status={status} onStop={props.onStop} />;
}

function Assurances() {
  const items = ["Runs on this machine only", "Protected by an API key", "Files verified by SHA256"];
  return (
    <ul className="flex flex-wrap gap-x-5 gap-y-1 text-sm text-white/70">
      {items.map((t) => (
        <li key={t} className="flex items-center gap-1.5">
          <span className="text-[#76b900]">✓</span>
          {t}
        </li>
      ))}
    </ul>
  );
}

function DownloadBar({ status, onCancel }: { status: Extract<Status, { stage: "downloading" }>; onCancel: () => void }) {
  // Velocidad suavizada a partir de los avisos de progreso.
  const sample = useRef<{ t: number; bytes: number; label: string; speed: number } | null>(null);
  const now = performance.now();
  const prev = sample.current;
  let speed = prev?.speed ?? 0;
  if (prev && prev.label === status.label && now - prev.t > 400) {
    const instant = ((status.downloaded - prev.bytes) * 1000) / (now - prev.t);
    speed = prev.speed ? prev.speed * 0.7 + instant * 0.3 : instant;
    sample.current = { t: now, bytes: status.downloaded, label: status.label, speed };
  } else if (!prev || prev.label !== status.label) {
    sample.current = { t: now, bytes: status.downloaded, label: status.label, speed: 0 };
    speed = 0;
  }
  const pct = (100 * status.downloaded) / Math.max(status.total, 1);
  const eta = speed > 0 ? (status.total - status.downloaded) / speed : null;

  return (
    <div className="flex flex-wrap items-end gap-x-6 gap-y-4">
      <div className="min-w-[16rem] flex-1">
        <div className="flex items-baseline justify-between">
          <div className="font-bold">
            Downloading {status.label}
            <span className="ml-2 text-sm font-normal text-white/55">
              file {status.step} of {status.steps}
            </span>
          </div>
          <div className="text-2xl font-bold tabular-nums">{pct.toFixed(0)}%</div>
        </div>
        <div className="mt-2 h-2 overflow-hidden rounded-full bg-white/10">
          <div className="shimmer h-full rounded-full bg-gradient-to-r from-[#2f8fff] to-[#76b900] transition-all duration-300" style={{ width: `${pct}%` }} />
        </div>
        <div className="mt-2 flex justify-between text-xs text-white/55 tabular-nums">
          <span>
            {formatBytes(status.downloaded)} of {formatBytes(status.total)}
            {speed > 0 && ` · ${formatBytes(speed)}/s`}
          </span>
          <span>{eta !== null ? `${formatDuration(eta)} left` : "Verified with SHA256"}</span>
        </div>
      </div>
      <GhostButton onClick={onCancel}>Cancel</GhostButton>
    </div>
  );
}

function LiveBar({ status, onStop }: { status: Extract<Status, { stage: "ready" }>; onStop: () => void }) {
  const since = useRef(Date.now());
  const [, tick] = useState(0);
  useEffect(() => {
    const id = window.setInterval(() => tick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, []);

  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-4">
      <div className="min-w-[16rem] flex-1">
        <div className="flex items-center gap-2 text-sm text-white/60">
          <Icon d="M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z" />
          Private server · only this machine can reach it
        </div>
        <div className="mt-1 flex items-baseline gap-3">
          <code className="font-mono text-xl font-bold text-[#b6f04a] select-text">{status.url}</code>
          <span className="text-sm text-white/50 tabular-nums">up {formatDuration((Date.now() - since.current) / 1000)}</span>
        </div>
      </div>
      <button
        onClick={onStop}
        className="flex items-center gap-2 rounded-xl border border-red-400/40 px-5 py-2.5 font-bold text-red-200 transition hover:border-red-400 hover:bg-red-500/20"
      >
        <span className="h-3 w-3 rounded-sm bg-current" />
        Stop server
      </button>
    </div>
  );
}

function Stepper({ status }: { status: Status }) {
  const current = { idle: -1, error: -1, downloading: 0, extracting: 1, starting: 2, ready: 4 }[status.stage];
  return (
    <ol className="grid grid-cols-2 gap-x-2 gap-y-3 @xl:grid-cols-4">
      {STAGES.map((label, i) => {
        const done = i < current || (status.stage === "starting" && i === 1);
        const now = i === current;
        return (
          <li key={label} className="flex items-center gap-2">
            <span
              className={`grid h-6 w-6 shrink-0 place-items-center rounded-full text-[11px] font-bold transition ${
                done ? "bg-go text-white" : now ? "bg-accent text-on-accent ring-4 ring-accent-soft" : "bg-surface-2 text-muted"
              }`}
            >
              {done ? "✓" : i + 1}
            </span>
            <span className={`text-sm font-bold ${done || now ? "text-text" : "text-muted"}`}>{label}</span>
            {i < STAGES.length - 1 && <span className={`h-px flex-1 ${done ? "bg-go" : "bg-border"}`} />}
          </li>
        );
      })}
    </ol>
  );
}

function ReadyCards({ status, modelName }: { status: Extract<Status, { stage: "ready" }>; modelName: string }) {
  const [copied, setCopied] = useState<string | null>(null);
  const [reveal, setReveal] = useState(false);
  const copy = (label: string, value: string) => {
    navigator.clipboard.writeText(value);
    setCopied(label);
    window.setTimeout(() => setCopied((c) => (c === label ? null : c)), 1500);
  };
  const masked = `${status.api_key.slice(0, 9)}${"•".repeat(16)}${status.api_key.slice(-4)}`;

  return (
    <div className="grid grid-cols-1 gap-5 @3xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
      <div className="flex flex-col rounded-2xl border border-border bg-surface p-6">
        <IconTile className="from-[#0672cb] to-[#2f8fff]">
          <Icon d="M4 5h16v11H8l-4 4z" />
        </IconTile>
        <h3 className="mt-4 text-lg font-bold">Chat</h3>
        <p className="mt-1 flex-1 text-sm text-muted">
          Talk to {modelName} in your browser. The first time it asks for the API key: paste it once and it is remembered.
        </p>
        <button
          onClick={() => openUrl(status.url)}
          className="mt-5 flex items-center justify-center gap-2 rounded-xl bg-accent px-5 py-3 font-bold text-on-accent shadow-lg shadow-accent/25 transition hover:-translate-y-0.5 hover:bg-accent-hover"
        >
          Open chat
          <Icon d="M7 17 17 7M9 7h8v8" />
        </button>
      </div>

      <div className="rounded-2xl border border-border bg-surface p-6">
        <div className="flex items-start gap-4">
          <IconTile className="from-[#3f8f00] to-[#76b900]">
            <Icon d="M10 14a4 4 0 0 0 5.66 0l3-3a4 4 0 0 0-5.66-5.66l-1 1M14 10a4 4 0 0 0-5.66 0l-3 3a4 4 0 0 0 5.66 5.66l1-1" />
          </IconTile>
          <div>
            <h3 className="text-lg font-bold">Connect apps</h3>
            <p className="mt-1 text-sm text-muted">Hermes or any OpenAI-compatible app.</p>
          </div>
        </div>

        <Field label="Endpoint URL">
          <code className="flex-1 truncate font-mono text-sm select-text">{status.api_url}</code>
          <CopyButton label="Copy endpoint URL" done={copied === "url"} onClick={() => copy("url", status.api_url)} />
        </Field>
        <Field label="API key">
          <code className="flex-1 truncate font-mono text-sm select-text">{reveal ? status.api_key : masked}</code>
          <button
            onClick={() => setReveal(!reveal)}
            aria-label={reveal ? "Hide API key" : "Show API key"}
            className="grid h-8 w-8 place-items-center rounded-lg text-muted hover:bg-surface hover:text-text"
          >
            <Icon d={reveal ? "M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.1A10 10 0 0 1 12 5c5 0 9 5 9 7a11 11 0 0 1-2.2 3.2M6.1 6.1C3.9 7.6 3 10 3 12c0 2 4 7 9 7a9.6 9.6 0 0 0 4.2-1" : "M3 12c0-2 4-7 9-7s9 5 9 7-4 7-9 7-9-5-9-7zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z"} />
          </button>
          <CopyButton label="Copy API key" done={copied === "key"} onClick={() => copy("key", status.api_key)} />
        </Field>

        <details className="group mt-4 text-sm">
          <summary className="cursor-pointer list-none font-bold text-accent hover:text-accent-hover">
            <span className="inline-block transition group-open:rotate-90">▸</span> Set up Hermes
          </summary>
          <ol className="mt-3 space-y-2">
            {[
              <>
                Run <code className="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-xs">hermes setup</code>
              </>,
              <>Choose “Custom OpenAI-compatible endpoint”.</>,
              <>Paste the endpoint URL, then the API key. Both stay the same every time you run a model.</>,
            ].map((step, i) => (
              <li key={i} className="flex gap-3 text-muted">
                <span className="grid h-5 w-5 shrink-0 place-items-center rounded-full bg-accent-soft text-[11px] font-bold text-accent">{i + 1}</span>
                <span>{step}</span>
              </li>
            ))}
          </ol>
        </details>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="mt-4">
      <div className="text-xs font-bold tracking-wider text-muted uppercase">{label}</div>
      <div className="mt-1.5 flex items-center gap-2 rounded-xl border border-border bg-surface-2 py-1.5 pr-1.5 pl-4">{children}</div>
    </div>
  );
}

function CopyButton({ label, done, onClick }: { label: string; done: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={`flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-bold transition ${
        done ? "bg-go text-white" : "bg-surface text-text shadow-sm hover:text-accent"
      }`}
    >
      <Icon d={done ? "M5 12l5 5L20 7" : "M9 9h11v11H9zM5 15H4V4h11v1"} />
      {done ? "Copied" : label}
    </button>
  );
}

function ServerLog({ logs }: { logs: string[] }) {
  const [open, setOpen] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (open) end.current?.scrollIntoView({ block: "nearest" });
  }, [open, logs.length]);
  if (!logs.length) return null;

  return (
    <div className="overflow-hidden rounded-2xl border border-border bg-[#0b0d10]">
      <button onClick={() => setOpen(!open)} className="flex w-full items-center gap-3 px-5 py-3 text-left text-sm font-bold text-[#b8c2cf] hover:text-white">
        <Icon d="M4 17l6-5-6-5M12 19h8" />
        Server log
        <span className="font-normal text-[#6b7685]">{logs.length} lines</span>
        <span className={`ml-auto transition ${open ? "rotate-90" : ""}`}>▸</span>
      </button>
      {open && (
        <pre className="max-h-64 overflow-auto border-t border-white/10 px-5 py-4 font-mono text-xs leading-relaxed text-[#b8c2cf] select-text">
          {logs.join("\n")}
          <div ref={end} />
        </pre>
      )}
    </div>
  );
}

function GhostButton({ onClick, children }: { onClick: () => void; children: ReactNode }) {
  return (
    <button onClick={onClick} className="rounded-xl border border-white/20 px-5 py-2.5 font-bold text-white/80 transition hover:border-white/50 hover:text-white">
      {children}
    </button>
  );
}

function IconTile({ className, children }: { className: string; children: ReactNode }) {
  return <div className={`grid h-11 w-11 shrink-0 place-items-center rounded-xl bg-gradient-to-br text-white shadow-md ${className}`}>{children}</div>;
}

function Icon({ d, fill }: { d: string; fill?: boolean }) {
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5" aria-hidden fill={fill ? "currentColor" : "none"} stroke={fill ? "none" : "currentColor"} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <path d={d} />
    </svg>
  );
}

function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
}
