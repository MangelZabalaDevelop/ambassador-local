import { useEffect, useRef, useState, type ReactNode } from "react";
import { api, formatBytes } from "../api";
import type { UninstallDone, UninstallFailure, UninstallInfo } from "../types";

type Phase =
  | { kind: "loading" }
  | { kind: "ready"; info: UninstallInfo }
  | { kind: "working"; info: UninstallInfo }
  | { kind: "failed"; info: UninstallInfo | null; failure: UninstallFailure }
  | { kind: "done"; done: UninstallDone };

const WHAT_HAPPENS: Record<NonNullable<UninstallInfo["method"]>, (info: UninstallInfo) => string> = {
  windows: () => "Ambassador Local closes and is removed from this computer, along with its Start menu shortcut.",
  deb: (info) => `Ambassador Local closes and its package (${info.package ?? "ambassador-local"}) is removed.${info.asks_password ? " Ubuntu asks for your password." : ""}`,
  appimage: () => "Ambassador Local closes and its AppImage file is deleted.",
};

/**
 * Desinstalar: confirma qué se quita, deja elegir si también los modelos y datos, y muestra el resultado.
 * Si algo no se puede hacer desde aquí, da el comando para terminarlo a mano.
 */
export function UninstallDialog(props: { onClose: () => void; busy: boolean; running: boolean; linux: boolean; linkedGb10: boolean }) {
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [deleteData, setDeleteData] = useState(true);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const working = phase.kind === "working" || (phase.kind === "done" && phase.done.closing);

  useEffect(() => {
    api
      .uninstallInfo()
      .then((info) => setPhase({ kind: "ready", info }))
      .catch((e) => setPhase({ kind: "failed", info: null, failure: { message: String(e), command: null } }));
  }, []);

  // Escape cierra, salvo mientras desinstala.
  const onClose = useRef(props.onClose);
  onClose.current = props.onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !working && onClose.current();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [working]);

  // El foco empieza en "Cancel": en un diálogo que borra, lo seguro es lo de por defecto.
  useEffect(() => {
    if (phase.kind === "ready") cancelRef.current?.focus();
  }, [phase.kind]);

  const run = (info: UninstallInfo) => {
    setPhase({ kind: "working", info });
    api
      .uninstall(deleteData)
      .then((done) => setPhase({ kind: "done", done }))
      .catch((failure: UninstallFailure) => setPhase({ kind: "failed", info, failure }));
  };

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center bg-black/50 p-4 backdrop-blur-[2px]"
      onMouseDown={(e) => e.target === e.currentTarget && !working && props.onClose()}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="uninstall-title"
        className="max-h-full w-full max-w-lg overflow-y-auto rounded-2xl border border-border bg-surface p-6 shadow-2xl"
      >
        <div className="flex items-center gap-3">
          <span className="grid h-10 w-10 shrink-0 place-items-center rounded-xl bg-red-500/10 text-red-600 dark:text-red-400">
            <TrashIcon />
          </span>
          <h2 id="uninstall-title" className="text-xl font-bold">
            Uninstall Ambassador Local
          </h2>
        </div>

        {phase.kind === "loading" && (
          <p className="mt-5 flex items-center gap-3 text-sm text-muted">
            <Spinner /> Checking how it was installed…
          </p>
        )}

        {phase.kind === "ready" && !phase.info.method && (
          <>
            <p className="mt-5 text-sm text-muted">{phase.info.reason}</p>
            <Buttons>
              <button ref={cancelRef} onClick={props.onClose} className={SECONDARY}>
                Close
              </button>
            </Buttons>
          </>
        )}

        {phase.kind === "ready" && phase.info.method && (
          <>
            <p className="mt-5 text-sm">{WHAT_HAPPENS[phase.info.method](phase.info)}</p>
            {props.running && <p className="mt-2 text-sm text-muted">The running model stops first.</p>}
            <label className="mt-5 flex cursor-pointer items-start gap-3 rounded-xl border border-border bg-surface-2 p-4 text-sm select-none">
              <input type="checkbox" checked={deleteData} onChange={(e) => setDeleteData(e.target.checked)} className="mt-0.5 h-4 w-4 shrink-0 accent-red-600" />
              <span>
                <span className="font-bold">Also delete downloaded models and settings</span>
                <span className="mt-0.5 block text-muted">
                  {phase.info.data_bytes > 0 ? `Frees ${formatBytes(phase.info.data_bytes)}. ` : ""}
                  If you install it again, models download again.
                </span>
              </span>
            </label>
            {deleteData && props.linux && (
              <p className="mt-3 text-xs text-muted">
                Model weights in the Hugging Face cache and Docker images stay: other tools may use them.
                {props.linkedGb10 && " Files on the second GB10 stay there too."}
              </p>
            )}
            {props.busy && <p className="mt-4 text-sm font-bold text-amber-700 dark:text-amber-300">Available when the current download or start finishes.</p>}
            <Buttons>
              <button ref={cancelRef} onClick={props.onClose} className={SECONDARY}>
                Cancel
              </button>
              <button disabled={props.busy} onClick={() => run(phase.info)} className={DANGER}>
                Uninstall
              </button>
            </Buttons>
          </>
        )}

        {phase.kind === "working" && (
          <div className="mt-5" role="status">
            <p className="flex items-center gap-3 text-sm font-bold">
              <Spinner /> Uninstalling…
            </p>
            {phase.info.asks_password && <p className="mt-2 text-sm text-muted">Enter your password in the window that opens.</p>}
          </div>
        )}

        {phase.kind === "failed" && (
          <>
            <p role="alert" className="mt-5 text-sm text-red-700 dark:text-red-300">
              {phase.failure.message}
            </p>
            {phase.failure.command && <CommandBox command={phase.failure.command} />}
            <Buttons>
              <button onClick={props.onClose} className={SECONDARY}>
                Close
              </button>
              {phase.info?.method && (
                <button disabled={props.busy} onClick={() => run(phase.info!)} className={DANGER}>
                  Try again
                </button>
              )}
            </Buttons>
          </>
        )}

        {phase.kind === "done" && phase.done.closing && (
          <p role="status" className="mt-5 flex items-center gap-3 text-sm font-bold text-go">
            <CheckIcon /> Ambassador Local was uninstalled. Closing…
          </p>
        )}

        {phase.kind === "done" && !phase.done.closing && (
          <>
            <p className="mt-5 flex items-center gap-3 text-sm font-bold text-go">
              <CheckIcon /> Ambassador Local was uninstalled.
            </p>
            <p className="mt-3 text-sm text-muted">Some files could not be deleted, probably because Docker created them as root:</p>
            <ul className="mt-2 space-y-1 font-mono text-xs break-all select-text">
              {phase.done.leftovers.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
            {phase.done.command && <CommandBox command={phase.done.command} />}
            <Buttons>
              <button autoFocus onClick={() => api.quit()} className={SECONDARY}>
                Close app
              </button>
            </Buttons>
          </>
        )}
      </div>
    </div>
  );
}

const SECONDARY = "rounded-xl border border-border px-5 py-2.5 text-sm font-bold transition hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none";
const DANGER =
  "rounded-xl bg-red-600 px-5 py-2.5 text-sm font-bold text-white shadow-lg shadow-red-600/25 transition hover:bg-red-700 focus-visible:ring-2 focus-visible:ring-red-400 focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50";

function Buttons({ children }: { children: ReactNode }) {
  return <div className="mt-6 flex flex-wrap justify-end gap-3">{children}</div>;
}

/** El comando para terminarlo a mano, con su botón de copiar. */
function CommandBox({ command }: { command: string }) {
  const [copied, setCopied] = useState<"done" | "failed" | null>(null);
  const copy = () =>
    Promise.resolve()
      .then(() => navigator.clipboard.writeText(command))
      .then(
        () => setCopied("done"),
        () => setCopied("failed"),
      );
  return (
    <div className="mt-3 flex items-center gap-2 rounded-xl border border-border bg-surface-2 py-1.5 pr-1.5 pl-4">
      <code className="flex-1 font-mono text-sm break-all select-text">{command}</code>
      <button
        onClick={copy}
        className={`shrink-0 rounded-lg px-3 py-1.5 text-sm font-bold transition ${copied === "done" ? "bg-go text-white" : copied === "failed" ? "bg-red-600 text-white" : "bg-surface text-text shadow-sm hover:text-accent"}`}
      >
        {copied === "done" ? "Copied" : copied === "failed" ? "Copy failed" : "Copy"}
      </button>
    </div>
  );
}

function Spinner() {
  return <span className="h-4 w-4 shrink-0 animate-spin rounded-full border-2 border-muted/30 border-t-accent" />;
}

function TrashIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M4 7h16M10 11v6M14 11v6M5 7l1 13h12l1-13M9 7V4h6v3" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg viewBox="0 0 24 24" className="h-5 w-5 shrink-0" fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M5 12l5 5L20 7" />
    </svg>
  );
}
