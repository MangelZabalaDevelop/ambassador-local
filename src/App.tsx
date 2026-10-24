import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "./api";
import { downloadKey, type Catalog, type Hardware, type Status, type System } from "./types";
import {
  HardwareImage,
  LabLogo,
  labLogoSrc,
  LinuxLogo,
  Watermark,
  WindowsLogo,
} from "./components/Illustrations";
import { ModelTable } from "./components/ModelTable";
import { RunScreen } from "./components/RunScreen";
import { SpecsModal } from "./components/SpecsModal";
import { UpdateBox } from "./components/UpdateBox";
import { useTheme, type ThemePref } from "./theme";
import logo from "./assets/ai-factory-logo-dark.webp";

const STEPS = [
  { label: "System", title: "Which operating system are you on?" },
  { label: "Hardware", title: "Choose your workstation" },
  { label: "Lab", title: "Choose a lab" },
  { label: "Model", title: "Choose a model" },
  { label: "Run", title: "Ready when you are" },
] as const;

export default function App() {
  const [themePref, setThemePref] = useTheme();
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [system, setSystem] = useState<System | null>(null);
  const [downloaded, setDownloaded] = useState<string[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [step, setStep] = useState(0);
  const [os, setOs] = useState<string | null>(null);
  const [hardwareId, setHardwareId] = useState<string | null>(null);
  const [familyId, setFamilyId] = useState<string | null>(null);
  const [modelId, setModelId] = useState<string | null>(null);

  const [status, setStatus] = useState<Status>({ stage: "idle" });
  const [logs, setLogs] = useState<string[]>([]);
  const [specsFor, setSpecsFor] = useState<Hardware | null>(null);
  const specsTimer = useRef<number | undefined>(undefined);
  // La ficha sale tras dejar el ratón quieto un momento sobre el "?", no al instante.
  const peekSpecs = (h: Hardware | null) => {
    window.clearTimeout(specsTimer.current);
    if (h) specsTimer.current = window.setTimeout(() => setSpecsFor(h), 1000);
    else setSpecsFor(null);
  };

  useEffect(() => {
    Promise.all([
      api.catalog(),
      api.detect(),
      api.downloaded(),
      api.currentStatus(),
    ])
      .then(([c, s, d, [st, l]]) => {
        setCatalog(c);
        setSystem(s);
        setDownloaded(d);
        setStatus(st);
        setLogs(l);
        setOs(s.os);
        setHardwareId(s.suggested_hardware);
      })
      .catch((e) => setLoadError(String(e)));
    const unStatus = api.onStatus((s) => {
      setStatus(s);
      if (s.stage === "ready") api.downloaded().then(setDownloaded);
    });
    const unLog = api.onLog((line) => setLogs((l) => [...l.slice(-399), line]));
    return () => {
      unStatus.then((f) => f());
      unLog.then((f) => f());
    };
  }, []);

  const hardware = catalog?.hardware.find((h) => h.id === hardwareId) ?? null;
  const family = catalog?.families.find((f) => f.id === familyId) ?? null;
  const model = catalog?.models.find((m) => m.id === modelId) ?? null;

  // Only families/models that have a recipe for the selected hardware.
  const models = useMemo(
    () =>
      catalog && hardwareId
        ? catalog.models.filter((m) => m.recipes[hardwareId])
        : [],
    [catalog, hardwareId],
  );
  const labModels = useMemo(
    () => models.filter((m) => m.family === familyId),
    [models, familyId],
  );

  if (loadError) return <Centered>Could not start: {loadError}</Centered>;
  if (!catalog || !system)
    return <Centered>Detecting your workstation…</Centered>;

  const canGo = [!!os, !!hardware, !!family, !!model, true];
  const busy =
    status.stage === "downloading" ||
    status.stage === "extracting" ||
    status.stage === "starting";
  const choices = [
    os === "windows" ? "Windows" : os === "linux" ? "Linux" : null,
    hardware?.name,
    family?.name,
    model?.name,
    null,
  ];

  return (
    <div className="flex h-full">
      <Sidebar
        step={step}
        choices={choices}
        busy={busy}
        running={status.stage === "ready"}
        onStep={setStep}
        themePref={themePref}
        onTheme={setThemePref}
        engine={catalog.engine.version}
      />

      <div className="relative flex min-w-0 flex-1 flex-col">
        <Watermark />
        <main className="relative flex-1 overflow-x-hidden overflow-y-auto px-6 py-8 lg:px-12 lg:py-10">
          <div className="mx-auto w-full max-w-5xl min-[1900px]:max-w-6xl">
            <div className="text-xs font-bold tracking-[0.18em] text-accent uppercase">
              Step {step + 1} of {STEPS.length}
            </div>
            <h1 className="mt-2 text-3xl font-bold tracking-tight">
              {STEPS[step].title}
            </h1>

            {step === 0 && (
              <Grid hint="We detected it for you.">
                {[
                  { id: "windows", name: "Windows", logo: <WindowsLogo /> },
                  { id: "linux", name: "Linux", logo: <LinuxLogo /> },
                ].map((o) => (
                  <Card
                    key={o.id}
                    selected={os === o.id}
                    disabled={o.id !== system.os}
                    badge={o.id === system.os ? "Detected" : "Not this machine"}
                    onClick={() => setOs(o.id)}
                  >
                    <div className="stage grid h-40 w-full place-items-center rounded-lg">
                      {o.logo}
                    </div>
                    <div className="text-lg font-bold">{o.name}</div>
                  </Card>
                ))}
              </Grid>
            )}

            {step === 1 && (
              <Grid
                // Tres por fila: así la GB10 y la GB10 ×2 quedan juntas.
                cols="grid-cols-2 lg:grid-cols-3"
                hint={
                  system.gpus.length
                    ? `GPU detected: ${system.gpus.map((g) => g.name).join(", ")}`
                    : "No NVIDIA GPU detected."
                }
              >
                {catalog.hardware.map((h) => {
                  // Solo se usa el equipo que se detecta en esta máquina.
                  const detected = system.detected_hardware.includes(h.id);
                  return (
                    <Card
                      key={h.id}
                      selected={hardwareId === h.id}
                      disabled={!detected}
                      badge={
                        !h.available
                          ? "Coming soon"
                          : detected
                            ? "Detected"
                            : "Not detected"
                      }
                      onClick={() => {
                        setHardwareId(h.id);
                        setFamilyId(null);
                        setModelId(null);
                      }}
                      corner={
                        <span
                          tabIndex={0}
                          aria-label={`${h.name} specifications`}
                          onMouseEnter={() => peekSpecs(h)}
                          onMouseLeave={() => peekSpecs(null)}
                          onFocus={() => peekSpecs(h)}
                          onBlur={() => peekSpecs(null)}
                          className="grid h-7 w-7 cursor-help place-items-center rounded-full border border-border bg-surface-2 text-sm font-bold text-muted transition outline-none hover:border-accent hover:bg-accent hover:text-on-accent focus-visible:ring-2 focus-visible:ring-accent"
                        >
                          ?
                        </span>
                      }
                    >
                      <HardwareImage hardware={h} />
                      <div className="w-full text-left">
                        <div className="font-bold">{h.name}</div>
                        <div className="mt-1 text-sm text-muted">{h.gpu}</div>
                      </div>
                      <div className="flex w-full gap-2">
                        <Chip>{h.memory_gb} GB</Chip>
                        <Chip>
                          {h.units > 1
                            ? `${h.units} linked`
                            : h.kind === "laptop"
                              ? "Mobile"
                              : h.kind === "tower"
                                ? "Tower"
                                : "Compact"}
                        </Chip>
                      </div>
                    </Card>
                  );
                })}
              </Grid>
            )}

            {step === 2 && (
              <Grid
                hint={`Labs with models tuned for your ${hardware?.name ?? "workstation"}. More are added over time.`}
              >
                {catalog.families.map((f) => {
                  const count = models.filter((m) => m.family === f.id).length;
                  return (
                    <Card
                      key={f.id}
                      selected={familyId === f.id}
                      disabled={count === 0}
                      badge={count === 0 ? "Coming soon" : undefined}
                      onClick={() => {
                        setFamilyId(f.id);
                        setModelId(null);
                      }}
                    >
                      <div className="mt-5 flex w-full items-center gap-4 text-left">
                        <LabLogo id={f.id} name={f.name} color={f.color} crowned={f.featured} />
                        <div className="min-w-0">
                          <div className="truncate text-lg font-bold">{f.name}</div>
                          <div
                            className="text-sm font-bold"
                            style={{ color: f.color }}
                          >
                            {f.series}
                          </div>
                        </div>
                      </div>
                      <p className="w-full text-left text-sm text-muted">
                        {f.description}
                      </p>
                      <div className="flex w-full flex-wrap items-center gap-2 text-left">
                        <Chip>
                          {count === 0
                            ? "No models yet"
                            : `${count} model${count > 1 ? "s" : ""} for you`}
                        </Chip>
                        {f.featured && (
                          <span className="rounded-md bg-gradient-to-r from-amber-300 to-amber-500 px-2 py-1 text-xs font-bold text-amber-950 shadow-sm">
                            Recommended
                          </span>
                        )}
                      </div>
                    </Card>
                  );
                })}
              </Grid>
            )}

            {step === 3 && hardwareId && family && (
              <ModelTable
                family={family.name}
                models={labModels}
                hardwareId={hardwareId}
                selected={modelId}
                downloaded={downloaded}
                onSelect={setModelId}
              />
            )}

            {step === 4 && hardware && model && (
              <RunScreen
                hardware={hardware}
                model={model}
                downloaded={downloaded.includes(downloadKey(model, hardware.id))}
                status={status}
                lab={catalog.families.find((f) => f.id === model.family)}
                logs={logs}
                onRun={() => api.start(hardware.id, model.id).catch(() => {})}
                onCancel={() => api.cancel()}
                onStop={() => api.stop()}
              />
            )}
          </div>
        </main>

        <footer className="relative border-t border-border bg-surface/85 px-6 py-4 backdrop-blur lg:px-12">
          <div className="mx-auto flex w-full max-w-5xl min-[1900px]:max-w-6xl items-center gap-4">
            <BackButton disabled={step === 0 || busy} onClick={() => setStep(step - 1)} />
            {step < 4 ? (
              <ContinueButton disabled={!canGo[step]} onClick={() => setStep(step + 1)} />
            ) : (
              <>
                {status.stage === "ready" && (
                  <span className="hidden items-center gap-2 rounded-full bg-go-soft px-3 py-1 text-xs font-bold text-go xl:flex">
                    <span className="pulse-dot h-1.5 w-1.5 rounded-full bg-current" />
                    Server stays live while you browse
                  </span>
                )}
                <div
                  className={`ml-auto flex items-center gap-1 rounded-2xl border border-border bg-surface-2 p-1 ${busy ? "opacity-40" : ""}`}
                  title={busy ? "Available when this finishes" : undefined}
                >
                  <SwitchChip
                    label="Lab"
                    value={family?.name ?? "—"}
                    icon={
                      family && labLogoSrc(family.id) ? (
                        <img src={labLogoSrc(family.id)} alt="" className="h-4 w-4" draggable={false} />
                      ) : (
                        <span className="text-xs font-bold">{family?.name[0]}</span>
                      )
                    }
                    disabled={busy}
                    onClick={() => setStep(2)}
                  />
                  <span className="h-8 w-px bg-border" />
                  <SwitchChip
                    label="Model"
                    value={model?.name ?? "—"}
                    icon={
                      <svg viewBox="0 0 24 24" className="h-4 w-4 text-accent" fill="none" stroke="currentColor" strokeWidth={2} strokeLinejoin="round">
                        <path d="M12 3 4 7.5v9L12 21l8-4.5v-9zM4 7.5 12 12l8-4.5M12 12v9" />
                      </svg>
                    }
                    disabled={busy}
                    onClick={() => setStep(3)}
                  />
                </div>
              </>
            )}
          </div>
        </footer>
      </div>

      {specsFor && <SpecsModal hardware={specsFor} />}
    </div>
  );
}

const ARROW = "M5 12h14M13 6l6 6-6 6";

function BackButton({ disabled, onClick }: { disabled: boolean; onClick: () => void }) {
  return (
    <button
      disabled={disabled}
      onClick={onClick}
      className="group flex items-center gap-3 rounded-full py-1 pr-4 pl-1 font-bold text-muted transition hover:text-text disabled:invisible"
    >
      <span className="grid h-9 w-9 place-items-center rounded-full border border-border bg-surface transition group-hover:border-text/30 group-hover:shadow-md">
        <svg viewBox="0 0 24 24" className="h-4 w-4 rotate-180 transition group-hover:-translate-x-0.5" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round">
          <path d={ARROW} />
        </svg>
      </span>
      Back
    </button>
  );
}

function ContinueButton({ disabled, onClick }: { disabled: boolean; onClick: () => void }) {
  return (
    <button
      disabled={disabled}
      onClick={onClick}
      className="group ml-auto flex items-center gap-3 rounded-full bg-accent py-1.5 pr-1.5 pl-6 font-bold text-on-accent shadow-lg shadow-accent/30 transition hover:-translate-y-px hover:bg-accent-hover hover:shadow-xl hover:shadow-accent/35 disabled:translate-y-0 disabled:bg-surface-2 disabled:text-muted disabled:shadow-none"
    >
      Continue
      <span className="grid h-8 w-8 place-items-center rounded-full bg-white/20 transition group-hover:translate-x-0.5 group-disabled:bg-border/60">
        <svg viewBox="0 0 24 24" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round">
          <path d={ARROW} />
        </svg>
      </span>
    </button>
  );
}

/** Píldora "cambiar": muestra la elección actual y lleva a ese paso. */
function SwitchChip(props: { label: string; value: string; icon: ReactNode; disabled: boolean; onClick: () => void }) {
  return (
    <button
      disabled={props.disabled}
      onClick={props.onClick}
      className="group flex items-center gap-3 rounded-xl py-1.5 pr-3 pl-1.5 text-left transition hover:bg-surface hover:shadow-md disabled:cursor-not-allowed disabled:hover:bg-transparent disabled:hover:shadow-none"
    >
      <span className="grid h-8 w-8 place-items-center rounded-lg border border-border bg-white">{props.icon}</span>
      <span className="leading-tight">
        <span className="block text-[10px] font-bold tracking-wider text-muted uppercase">{props.label}</span>
        <span className="block max-w-40 truncate text-sm font-bold">{props.value}</span>
      </span>
      <svg viewBox="0 0 24 24" aria-label="Change" className="ml-1 h-4 w-4 text-muted transition group-hover:text-accent group-disabled:text-muted" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
        <path d="M7 4 3 8l4 4M3 8h14M17 20l4-4-4-4M21 16H7" />
      </svg>
    </button>
  );
}

function Sidebar(props: {
  step: number;
  choices: (string | null | undefined)[];
  busy: boolean;
  running: boolean;
  onStep: (i: number) => void;
  themePref: ThemePref;
  onTheme: (t: ThemePref) => void;
  engine: string;
}) {
  return (
    <aside className="flex w-60 shrink-0 flex-col overflow-y-auto bg-sidebar px-4 py-6 text-sidebar-text [scrollbar-width:none] xl:w-72 xl:px-6 xl:py-7 [@media(max-height:760px)]:py-4">
      <div className="flex items-center gap-3">
        <AppMark />
        <div>
          <div className="text-base leading-tight font-bold whitespace-nowrap xl:text-lg">
            Ambassador Local
          </div>
          <div className="text-xs text-sidebar-muted">
            Local AI for Dell Pro Precision Ambassadors
          </div>
        </div>
      </div>

      <nav className="mt-8 flex flex-col gap-1 xl:mt-10 [@media(max-height:760px)]:mt-5">
        {STEPS.map((s, i) => {
          const done = i < props.step;
          const current = i === props.step;
          return (
            <button
              key={s.label}
              disabled={props.busy || i > props.step}
              onClick={() => props.onStep(i)}
              className={`flex items-center gap-3 rounded-lg px-3 py-2.5 text-left transition [@media(max-height:760px)]:py-1.5 ${current ? "bg-white/10" : "hover:bg-white/5"} disabled:hover:bg-transparent`}
            >
              <span
                className={`grid h-7 w-7 shrink-0 place-items-center rounded-full text-xs font-bold ${
                  current
                    ? "bg-accent text-on-accent"
                    : done
                      ? "bg-white/15 text-sidebar-text"
                      : "border border-white/20 text-sidebar-muted"
                }`}
              >
                {done ? "✓" : i + 1}
              </span>
              <span className="min-w-0">
                <span
                  className={`block text-sm font-bold ${current || done ? "" : "text-sidebar-muted"}`}
                >
                  {s.label}
                </span>
                {i === 4 && props.running ? (
                  <span className="block text-xs text-[#76b900]">
                    ● Running
                  </span>
                ) : (
                  props.choices[i] &&
                  i < props.step && (
                    <span className="block truncate text-xs text-sidebar-muted">
                      {props.choices[i]}
                    </span>
                  )
                )}
              </span>
            </button>
          );
        })}
      </nav>

      <div className="mt-auto flex flex-col items-center border-t border-white/10 pt-6 [@media(max-height:760px)]:pt-3">
        <div className="text-[10px] font-bold tracking-[0.25em] text-sidebar-muted uppercase">
          Powered by
        </div>
        <img
          src={logo}
          alt="Dell Technologies AI Factory with NVIDIA"
          className="mt-3 w-40 xl:w-48 [@media(max-height:760px)]:mt-2 [@media(max-height:760px)]:w-32"
          draggable={false}
        />
        <div className="mt-6 flex w-full rounded-lg bg-black/25 p-1 [@media(max-height:760px)]:mt-3">
          {(["light", "dark", "system"] as const).map((t) => (
            <button
              key={t}
              onClick={() => props.onTheme(t)}
              className={`flex-1 rounded-md py-1.5 text-xs font-bold capitalize transition ${
                props.themePref === t
                  ? "bg-white/15 text-sidebar-text"
                  : "text-sidebar-muted hover:text-sidebar-text"
              }`}
            >
              {t === "light" ? "☀ Light" : t === "dark" ? "☾ Dark" : "Auto"}
            </button>
          ))}
        </div>
        <div className="mt-4 text-center text-[11px] text-sidebar-muted [@media(max-height:760px)]:mt-2">
          llama.cpp {props.engine} · runs on 127.0.0.1 only
        </div>
        <UpdateBox busy={props.busy} running={props.running} />
      </div>
    </aside>
  );
}

function Grid({ hint, cols, children }: { hint?: string; cols?: string; children: ReactNode }) {
  return (
    <>
      {hint && <p className="mt-2 text-muted">{hint}</p>}
      <div className={`mt-8 grid gap-5 ${cols ?? "grid-cols-[repeat(auto-fill,minmax(230px,1fr))]"}`}>
        {children}
      </div>
    </>
  );
}

function Card(props: {
  selected: boolean;
  disabled?: boolean;
  badge?: string;
  onClick: () => void;
  corner?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div
      className={`group relative flex flex-col rounded-xl border bg-surface transition ${
        props.selected
          ? "border-accent shadow-lg ring-1 shadow-accent/15 ring-accent"
          : "border-border hover:-translate-y-0.5 hover:shadow-lg"
      }`}
    >
      <button
        disabled={props.disabled}
        onClick={props.onClick}
        className="flex flex-1 flex-col items-center gap-4 p-5 text-center disabled:cursor-not-allowed disabled:opacity-35"
      >
        {props.children}
      </button>
      {props.corner && (
        <div className="absolute right-3 bottom-3">{props.corner}</div>
      )}
      {props.badge && (
        <span
          className={`absolute top-3 left-3 rounded-full px-2.5 py-0.5 text-[11px] font-bold ${
            props.badge === "Detected" || props.badge === "Downloaded"
              ? "bg-accent text-on-accent"
              : "bg-surface-2 text-muted"
          }`}
        >
          {props.badge}
        </span>
      )}
      {props.selected && (
        <span className="absolute top-3 right-3 grid h-6 w-6 place-items-center rounded-full bg-accent text-xs font-bold text-on-accent">
          ✓
        </span>
      )}
    </div>
  );
}

function Chip({ children }: { children: ReactNode }) {
  return (
    <span className="rounded-md bg-surface-2 px-2 py-1 text-xs font-bold text-muted">
      {children}
    </span>
  );
}

/** Marca de la app: corchetes con degradado azul→verde y botón de "run". */
function AppMark() {
  return (
    <svg viewBox="0 0 48 48" className="h-10 w-10 shrink-0" aria-hidden>
      <defs>
        <linearGradient id="mark" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#2f8fff" />
          <stop offset="1" stopColor="#76b900" />
        </linearGradient>
      </defs>
      <path
        d="M17 6 H7 V42 H17 M31 6 H41 V42 H31"
        fill="none"
        stroke="url(#mark)"
        strokeWidth="4.5"
      />
      <path d="M19 15 L33 24 L19 33 Z" fill="url(#mark)" />
    </svg>
  );
}

function Centered({ children }: { children: ReactNode }) {
  return (
    <div className="grid h-full place-items-center text-muted">{children}</div>
  );
}
