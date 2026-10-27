import type { Hardware } from "../types";
import { HardwarePhoto } from "./Illustrations";

/**
 * Ficha del producto; aparece mientras el ratón está sobre el "?" de la tarjeta. Como no se puede desplazar
 * (es una ficha flotante, no una ventana), se compacta en ventanas bajas para que se vea entera.
 */
export function SpecsModal({ hardware }: { hardware: Hardware }) {
  const { specs } = hardware;

  return (
    <div className="pointer-events-none fixed inset-0 z-50 grid place-items-center bg-black/40 p-8 backdrop-blur-[2px] [@media(max-height:700px)]:p-4">
      <div
        role="tooltip"
        aria-label={`${hardware.name} specifications`}
        className="relative flex max-h-full w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-border bg-surface shadow-2xl"
      >
        <div className="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)] bg-sidebar text-sidebar-text lg:grid-cols-[minmax(0,5fr)_minmax(0,6fr)]">
          <div className="grid place-items-center stage p-8 [@media(max-height:900px)]:p-5">
            <HardwarePhoto hardware={hardware} className="max-h-64 drop-shadow-xl [@media(max-height:900px)]:max-h-40 [@media(max-height:700px)]:max-h-28" />
          </div>
          <div className="flex flex-col justify-center p-8 [@media(max-height:900px)]:px-7 [@media(max-height:900px)]:py-5">
            <div className="text-xs font-bold tracking-[0.2em] text-white/60 uppercase">Model {specs.model}</div>
            <h2 className="mt-1 text-3xl font-bold [@media(max-height:700px)]:text-2xl">{hardware.name}</h2>
            <p className="mt-3 text-white/80 [@media(max-height:800px)]:hidden">{specs.summary}</p>
            <div className="mt-6 grid grid-cols-3 gap-3 [@media(max-height:900px)]:mt-4">
              {specs.highlights.map((s) => (
                <div key={s.label} className="rounded-lg bg-white/10 p-3">
                  <div className="text-xl leading-tight font-bold whitespace-nowrap">{s.value}</div>
                  <div className="mt-1 text-xs text-white/70">{s.label}</div>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="overflow-y-auto p-8 [@media(max-height:900px)]:px-7 [@media(max-height:900px)]:py-4">
          <dl className="divide-y divide-border">
            {specs.details.map((s) => (
              <div key={s.label} className="grid grid-cols-[8rem_1fr] gap-4 py-3 lg:grid-cols-[10rem_1fr] [@media(max-height:900px)]:py-2 [@media(max-height:660px)]:py-1.5">
                <dt className="text-sm font-bold text-accent">{s.label}</dt>
                <dd className="text-sm">{s.value}</dd>
              </div>
            ))}
          </dl>
          {/* En ventanas muy bajas, "Best for" va en la misma línea que sus etiquetas. */}
          <div className="mt-6 [@media(max-height:900px)]:mt-3 [@media(max-height:660px)]:flex [@media(max-height:660px)]:flex-wrap [@media(max-height:660px)]:items-center [@media(max-height:660px)]:gap-x-3">
            <div className="text-sm font-bold text-muted">Best for</div>
            <div className="mt-2 flex flex-wrap gap-2 [@media(max-height:660px)]:mt-0">
              {specs.best_for.map((b) => (
                <span key={b} className="rounded-full bg-accent-soft px-3 py-1 text-sm font-bold text-accent [@media(max-height:660px)]:py-0.5">
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
