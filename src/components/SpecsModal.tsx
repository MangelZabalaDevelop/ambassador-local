import type { Hardware } from "../types";
import { HardwarePhoto } from "./Illustrations";

/** Ficha del producto; aparece mientras el ratón está sobre el "?" de la tarjeta. */
export function SpecsModal({ hardware }: { hardware: Hardware }) {
  const { specs } = hardware;

  return (
    <div className="pointer-events-none fixed inset-0 z-50 grid place-items-center bg-black/40 p-8 backdrop-blur-[2px]">
      <div
        role="tooltip"
        aria-label={`${hardware.name} specifications`}
        className="relative flex max-h-full w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-border bg-surface shadow-2xl"
      >
        <div className="grid grid-cols-[minmax(0,5fr)_minmax(0,6fr)] bg-sidebar text-sidebar-text">
          <div className="grid place-items-center stage p-8">
            <HardwarePhoto hardware={hardware} className="max-h-64 drop-shadow-xl" />
          </div>
          <div className="flex flex-col justify-center p-8">
            <div className="text-xs font-bold tracking-[0.2em] text-white/60 uppercase">Model {specs.model}</div>
            <h2 className="mt-1 text-3xl font-bold">{hardware.name}</h2>
            <p className="mt-3 text-white/80">{specs.summary}</p>
            <div className="mt-6 grid grid-cols-3 gap-3">
              {specs.highlights.map((s) => (
                <div key={s.label} className="rounded-lg bg-white/10 p-3">
                  <div className="text-xl leading-tight font-bold whitespace-nowrap">{s.value}</div>
                  <div className="mt-1 text-xs text-white/70">{s.label}</div>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="overflow-y-auto p-8">
          <dl className="divide-y divide-border">
            {specs.details.map((s) => (
              <div key={s.label} className="grid grid-cols-[10rem_1fr] gap-4 py-3">
                <dt className="text-sm font-bold text-accent">{s.label}</dt>
                <dd className="text-sm">{s.value}</dd>
              </div>
            ))}
          </dl>
          <div className="mt-6">
            <div className="text-sm font-bold text-muted">Best for</div>
            <div className="mt-2 flex flex-wrap gap-2">
              {specs.best_for.map((b) => (
                <span key={b} className="rounded-full bg-accent-soft px-3 py-1 text-sm font-bold text-accent">
                  {b}
                </span>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
