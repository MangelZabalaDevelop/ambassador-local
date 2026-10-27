import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { UpdateInfo } from "../types";

type State =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; info: UpdateInfo; confirm: boolean }
  | { kind: "installing"; info: UpdateInfo }
  | { kind: "error"; label: string; message: string };

/**
 * Actualización opcional en la barra lateral. Mira una vez al abrir (solo pregunta,
 * no descarga) y nunca instala sin que el embajador lo confirme.
 */
export function UpdateBox({ busy, running }: { busy: boolean; running: boolean }) {
  const [version, setVersion] = useState<string | null>(null);
  const [state, setState] = useState<State>({ kind: "idle" });

  const check = (quiet: boolean) => {
    setState({ kind: "checking" });
    api
      .checkUpdate()
      .then((info) => setState(info ? { kind: "available", info, confirm: false } : { kind: quiet ? "idle" : "latest" }))
      .catch((e) => setState(quiet ? { kind: "idle" } : { kind: "error", label: "Could not check", message: String(e) }));
  };

  useEffect(() => {
    api.version().then(setVersion, () => {});
    check(true);
  }, []);

  // Al pedir confirmación (o al instalar / fallar) la caja crece: que su botón quede a la vista aunque la ventana sea baja.
  const box = useRef<HTMLDivElement>(null);
  const confirming = state.kind === "available" && state.confirm;
  useEffect(() => {
    if (confirming || state.kind === "installing" || state.kind === "error") box.current?.scrollIntoView({ block: "nearest" });
  }, [confirming, state.kind]);

  const install = (info: UpdateInfo) => {
    setState({ kind: "installing", info });
    api.installUpdate().catch((e) => setState({ kind: "error", label: "Update failed", message: String(e) }));
  };

  if (state.kind === "available" || state.kind === "installing") {
    const { info } = state;
    return (
      <div ref={box} className="mt-5 w-full rounded-xl border border-[#76b900]/40 bg-[#76b900]/10 p-3 text-left [@media(max-height:760px)]:mt-3">
        <div className="flex items-center gap-2 text-sm font-bold">
          <span className="pulse-dot h-2 w-2 rounded-full bg-[#76b900]" />
          Version {info.version} is available
        </div>
        {/* Dos líneas, una en ventanas bajas (el texto entero queda en el title): así el botón Update se ve. */}
        {info.notes && (
          <p className="mt-1 line-clamp-2 text-xs text-sidebar-muted [@media(max-height:760px)]:line-clamp-1" title={info.notes}>
            {info.notes}
          </p>
        )}

        {state.kind === "installing" ? (
          <div className="mt-3 flex items-center gap-2 text-xs font-bold">
            <span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white/30 border-t-white" />
            Downloading and checking the signature…
          </div>
        ) : state.confirm ? (
          <>
            <p className="mt-2 text-xs text-sidebar-text">
              {running ? "The running model will stop and the " : "The "}app will restart. Your downloaded models stay.
            </p>
            <div className="mt-2 flex gap-2">
              <button onClick={() => install(info)} className="flex-1 rounded-lg bg-[#76b900] py-1.5 text-xs font-bold text-[#0e1111] transition hover:bg-[#8fd400]">
                Update and restart
              </button>
              <button onClick={() => setState({ ...state, confirm: false })} className="rounded-lg px-3 py-1.5 text-xs font-bold text-sidebar-muted hover:text-sidebar-text">
                Later
              </button>
            </div>
          </>
        ) : (
          <button
            disabled={busy}
            title={busy ? "Available when this finishes" : undefined}
            onClick={() => setState({ ...state, confirm: true })}
            className="mt-3 w-full rounded-lg bg-[#76b900] py-1.5 text-xs font-bold text-[#0e1111] transition hover:bg-[#8fd400] disabled:cursor-not-allowed disabled:opacity-40"
          >
            Update
          </button>
        )}
      </div>
    );
  }

  return (
    <div ref={box} className="mt-4 flex w-full items-center justify-between gap-2 text-[11px] text-sidebar-muted [@media(max-height:760px)]:mt-2">
      <span className="truncate" title={state.kind === "error" ? state.message : undefined}>
        {state.kind === "latest" ? "✓ Up to date" : state.kind === "error" ? state.label : version ? `Version ${version}` : ""}
      </span>
      <button
        disabled={state.kind === "checking"}
        onClick={() => check(false)}
        className="flex shrink-0 items-center gap-1.5 rounded-md px-2 py-1 font-bold transition hover:bg-white/10 hover:text-sidebar-text disabled:opacity-60"
      >
        <svg viewBox="0 0 24 24" className={`h-3.5 w-3.5 ${state.kind === "checking" ? "animate-spin" : ""}`} fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7" />
        </svg>
        {state.kind === "checking" ? "Checking…" : "Check for updates"}
      </button>
    </div>
  );
}
