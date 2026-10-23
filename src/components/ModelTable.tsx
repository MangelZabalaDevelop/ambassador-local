import { useState, type ReactNode } from "react";
import { downloadKey, type Model, type SizeClass } from "../types";

type Props = {
  family: string;
  models: Model[];
  hardwareId: string;
  selected: string | null;
  downloaded: string[];
  onSelect: (id: string) => void;
};

const SIZES: Record<SizeClass, { label: string; level: number; range: string }> = {
  small: { label: "Small", level: 1, range: "4–14B parameters" },
  medium: { label: "Medium", level: 2, range: "24–32B parameters" },
  large: { label: "Large", level: 3, range: "~70B, up to 96 GB of GPU memory" },
  xlarge: { label: "X-Large", level: 4, range: "Needs two linked GB10 / DGX Spark" },
};

const BOLT = "M13 2 4 14h7l-1 8 9-12h-7z";
const BULB = "M9 18h6M10 21h4M12 3a6 6 0 0 0-3.8 10.6c.6.5.8 1.2.8 2V16h6v-.4c0-.8.2-1.5.8-2A6 6 0 0 0 12 3z";
const BOOK = "M5 4.5A1.5 1.5 0 0 1 6.5 3H19v15H6.5A1.5 1.5 0 0 0 5 19.5zM5 19.5A1.5 1.5 0 0 0 6.5 21H19v-3";

/** Explicación de cada columna: se usa en la cabecera (ancho) y en las etiquetas de cada tarjeta (estrecho). */
const COLUMNS: { key: string; title: string; tip: ReactNode; legend?: ReactNode }[] = [
  {
    key: "size",
    title: "Size",
    tip: (
      <>
        How big the model is. Bigger models know more, but need more GPU memory.
        <ul className="mt-2 space-y-1">
          {Object.values(SIZES).map((s) => (
            <li key={s.label} className="flex items-center gap-2">
              <SizeBars level={s.level} />
              <span className="w-16 font-bold text-text">{s.label}</span>
              <span>{s.range}</span>
            </li>
          ))}
        </ul>
      </>
    ),
  },
  {
    key: "speed",
    title: "Speed",
    tip: <>How fast it answers on your workstation. Five bolts means replies appear almost instantly.</>,
    legend: <Rating value={5} d={BOLT} on="text-amber-400" />,
  },
  {
    key: "intelligence",
    title: "Intelligence",
    tip: <>How capable its answers are. More bulbs means smarter, but usually a bit slower.</>,
    legend: <Rating value={5} d={BULB} on="text-yellow-400" />,
  },
  {
    key: "memory",
    title: "Memory",
    tip: <>How much it can keep in mind at once: your conversation, documents or code. Measured in tokens; one token is about ¾ of a word.</>,
    legend: <Rating value={5} d={BOOK} on="text-accent" />,
  },
  { key: "best", title: "Best for", tip: <>What this model does best, so you can pick the right one for the job.</> },
  {
    key: "tps",
    title: "Tokens/s",
    tip: <>Average generation speed, measured by hand on this exact workstation. Around 10 tokens/s reads like a fast typist; 30+ feels instant.</>,
  },
];

// Ancho (contenedor ≥ 54rem, suma de los mínimos de columna + huecos): tabla. Estrecho: cada modelo es una tarjeta con sus métricas en rejilla.
const WIDE_COLS =
  "@min-[54rem]:grid-cols-[1.75rem_minmax(9rem,2.2fr)_minmax(5.5rem,1fr)_minmax(6.25rem,1fr)_minmax(6.25rem,1fr)_minmax(6.5rem,1.1fr)_minmax(5.5rem,1fr)_minmax(5.5rem,0.9fr)]";

/** Modelos para "dummies": todo en iconos, sin jerga. Los datos salen de recipes/catalog.json. */
export function ModelTable(props: Props) {
  return (
    <>
      <p className="mt-2 text-muted">{props.family} models tuned and tested for your workstation. Hover a title to learn what it means.</p>
      <div className="@container mt-8">
        <div className="rounded-2xl border border-border bg-surface shadow-sm">
          <div
            className={`hidden items-center gap-x-3 rounded-t-2xl border-b border-border bg-surface-2 px-4 py-3 text-xs font-bold text-muted @min-[54rem]:grid ${WIDE_COLS}`}
          >
            <span />
            <Tip title="Model" tip={<>The model you will run. Its name tells you the lab and version.</>} />
            {COLUMNS.map((c, i) => (
              <Tip key={c.key} title={c.title} tip={c.tip} legend={c.legend} align={i >= COLUMNS.length - 3 ? "right" : "left"} />
            ))}
          </div>

          <div className="divide-y divide-border">
            {props.models.map((m) => (
              <Row
                key={m.id}
                model={m}
                hardwareId={props.hardwareId}
                selected={props.selected === m.id}
                downloaded={props.downloaded.includes(downloadKey(m, props.hardwareId))}
                onSelect={() => props.onSelect(m.id)}
              />
            ))}
          </div>
        </div>
      </div>
    </>
  );
}

function Row(props: { model: Model; hardwareId: string; selected: boolean; downloaded: boolean; onSelect: () => void }) {
  const { model: m, selected } = props;
  const recipe = m.recipes[props.hardwareId];
  const size = SIZES[m.size_class];
  const cell = (key: string, children: ReactNode) => {
    const col = COLUMNS.find((c) => c.key === key)!;
    return (
      <div className="min-w-0">
        <div className="mb-1.5 text-xs font-bold text-muted @min-[54rem]:hidden">
          <Tip title={col.title} tip={col.tip} legend={col.legend} />
        </div>
        {children}
      </div>
    );
  };

  return (
    <div
      role="radio"
      aria-checked={selected}
      tabIndex={0}
      onClick={props.onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), props.onSelect())}
      className={`grid cursor-pointer grid-cols-[1.75rem_minmax(0,1fr)] items-start gap-x-3 gap-y-4 px-4 py-5 transition outline-none first:rounded-t-2xl last:rounded-b-2xl focus-visible:ring-2 focus-visible:ring-accent @min-[54rem]:items-center @min-[54rem]:gap-y-0 @min-[54rem]:first:rounded-t-none ${WIDE_COLS} ${
        selected ? "bg-accent-soft" : "hover:bg-surface-2"
      }`}
    >
      <span
        className={`mt-0.5 grid h-5 w-5 place-items-center rounded-full border-2 transition @min-[54rem]:mt-0 ${
          selected ? "border-accent bg-accent text-[10px] font-bold text-on-accent" : "border-muted/40"
        }`}
      >
        {selected && "✓"}
      </span>

      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-base font-bold">
          <span>{m.name}</span>
          {props.downloaded && <span className="rounded-full bg-go-soft px-2 py-0.5 text-[10px] font-bold text-go">Downloaded</span>}
        </div>
        <div className="mt-0.5 text-sm text-muted">{m.tagline}</div>
      </div>

      <div className="col-start-2 grid grid-cols-2 gap-x-4 gap-y-4 @md:grid-cols-3 @min-[54rem]:contents">
        {cell(
          "size",
          <>
            <div className="flex items-center gap-2">
              <SizeBars level={size.level} />
              <span className="font-bold">{size.label}</span>
            </div>
            <div className="mt-1 text-xs text-muted">{m.size} parameters</div>
          </>,
        )}
        {cell("speed", <Rating value={recipe.speed} d={BOLT} on="text-amber-400" label="Speed" />)}
        {cell("intelligence", <Rating value={m.intelligence} d={BULB} on="text-yellow-400" label="Intelligence" />)}
        {cell(
          "memory",
          <>
            <Rating value={memoryLevel(recipe.ctx_size)} d={BOOK} on="text-accent" label="Memory" />
            <div className="mt-1 text-xs text-muted tabular-nums">
              {(recipe.ctx_size / 1024).toFixed(0)}K · ~{formatWords(recipe.ctx_size * 0.75)} words
            </div>
          </>,
        )}
        {cell(
          "best",
          <div className="flex flex-wrap gap-1.5">
            {m.best_for.map((b) => (
              <span key={b} className="rounded-full bg-surface-2 px-2.5 py-1 text-xs font-bold whitespace-nowrap text-text/75">
                {b}
              </span>
            ))}
          </div>,
        )}
        {cell("tps", <TokensPerSec value={recipe.tokens_per_sec} />)}
      </div>
    </div>
  );
}

/** Título con una fichita explicativa al pasar el ratón (o al enfocarlo con el teclado). */
function Tip(props: { title: string; tip: ReactNode; legend?: ReactNode; align?: "left" | "right" }) {
  // Abrir la ficha hacia donde haya espacio, para que nunca se salga de la ventana.
  const [align, setAlign] = useState(props.align ?? "left");
  const place = (el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    setAlign(r.left + 368 > window.innerWidth ? "right" : "left");
  };
  return (
    <span className="group relative inline-flex" onMouseEnter={(e) => place(e.currentTarget)} onFocus={(e) => place(e.currentTarget)}>
      <span tabIndex={0} onClick={(e) => e.stopPropagation()} className="inline-flex cursor-help items-center gap-1.5 whitespace-nowrap outline-none hover:text-text focus-visible:text-text">
        {props.title}
        <svg viewBox="0 0 24 24" className="h-3.5 w-3.5 opacity-60" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" aria-hidden>
          <circle cx="12" cy="12" r="9" />
          <path d="M12 11v5M12 8h.01" />
        </svg>
      </span>
      <span
        role="tooltip"
        className={`pointer-events-none invisible absolute top-full z-30 mt-2 w-[min(22rem,80vw)] translate-y-1 rounded-xl border border-border bg-surface p-4 text-sm font-normal tracking-normal text-muted normal-case opacity-0 shadow-2xl transition delay-150 duration-200 group-focus-within:visible group-focus-within:translate-y-0 group-focus-within:opacity-100 group-hover:visible group-hover:translate-y-0 group-hover:opacity-100 ${
          align === "right" ? "right-0" : "left-0"
        }`}
      >
        <span className="mb-1 flex items-center justify-between">
          <span className="font-bold text-text">{props.title}</span>
          {props.legend}
        </span>
        <span className="block">{props.tip}</span>
      </span>
    </span>
  );
}

/** Promedio medido por el autor del catálogo; vacío si aún no se ha probado. */
function TokensPerSec({ value }: { value?: number | null }) {
  if (value == null) return <span className="text-xs text-muted">Not measured yet</span>;
  return (
    <div className="whitespace-nowrap">
      <span className="text-xl font-bold text-go tabular-nums">{value >= 100 ? Math.round(value) : value.toFixed(1).replace(/\.0$/, "")}</span>
      <span className="ml-1 text-xs font-bold text-muted">tok/s</span>
      <div className="mt-0.5 text-xs text-muted tabular-nums">~{Math.round(value * 0.75)} words/s</div>
    </div>
  );
}

function Rating(props: { value: number; d: string; on: string; label?: string }) {
  return (
    <span
      className="flex items-center gap-0.5"
      role={props.label ? "img" : undefined}
      aria-label={props.label ? `${props.label}: ${props.value} of 5` : undefined}
    >
      {Array.from({ length: 5 }, (_, i) => {
        const lit = i < props.value;
        return (
          <svg
            key={i}
            viewBox="0 0 24 24"
            className={`h-[18px] w-[18px] shrink-0 ${lit ? `${props.on} drop-shadow-[0_0_4px_currentColor]` : "text-muted/35"}`}
            fill={lit ? "currentColor" : "none"}
            stroke="currentColor"
            strokeWidth={1.8}
            strokeLinejoin="round"
            strokeLinecap="round"
            aria-hidden
          >
            <path d={props.d} />
          </svg>
        );
      })}
    </span>
  );
}

function SizeBars({ level }: { level: number }) {
  return (
    <span className="flex h-4 items-end gap-[3px]" aria-hidden>
      {[1, 2, 3, 4].map((n) => (
        <span key={n} className={`w-[5px] rounded-sm ${n <= level ? "bg-accent" : "bg-muted/30"}`} style={{ height: `${n * 25}%` }} />
      ))}
    </span>
  );
}

/** Contexto en tokens → libros (1 a 5). */
function memoryLevel(ctx: number): number {
  if (ctx <= 8192) return 1;
  if (ctx <= 16384) return 2;
  if (ctx <= 32768) return 3;
  if (ctx <= 65536) return 4;
  return 5;
}

function formatWords(n: number): string {
  return n >= 1000 ? `${Math.round(n / 1000)}K` : `${Math.round(n)}`;
}
