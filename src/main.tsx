import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

/** Si algo de la interfaz falla, mostrar el error con un botón para recargar en vez de una ventana en blanco. */
class ErrorBoundary extends React.Component<{ children: React.ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="grid h-full place-items-center p-8">
        <div className="max-w-lg rounded-2xl border border-border bg-surface p-8 text-center shadow-xl">
          <div className="text-xl font-bold">Something went wrong</div>
          <p className="mt-2 text-muted">The window hit an unexpected error. Your downloads are safe. Reload to continue.</p>
          <pre className="mt-4 max-h-32 overflow-auto rounded-lg bg-surface-2 p-3 text-left font-mono text-xs text-muted select-text">
            {this.state.error.message}
          </pre>
          <button
            onClick={() => location.reload()}
            className="mt-6 rounded-full bg-accent px-6 py-2.5 font-bold text-on-accent shadow-lg shadow-accent/30 hover:bg-accent-hover"
          >
            Reload
          </button>
        </div>
      </div>
    );
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
