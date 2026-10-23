import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ThemePref = "light" | "dark" | "system";

const KEY = "ambassador-local.theme";
const inTauri = "__TAURI_INTERNALS__" in window;

function readPref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    // Sin almacenamiento: usar el del sistema.
  }
  return "system";
}

/** Tema elegido por el usuario + tema efectivo; aplica la clase `.dark` y el tema de la ventana. */
export function useTheme(): [ThemePref, (p: ThemePref) => void, "light" | "dark"] {
  const [pref, setPref] = useState<ThemePref>(readPref);
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const [systemDark, setSystemDark] = useState(media.matches);

  useEffect(() => {
    const onChange = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [media]);

  const resolved = pref === "system" ? (systemDark ? "dark" : "light") : pref;

  useEffect(() => {
    document.documentElement.classList.toggle("dark", resolved === "dark");
    try {
      localStorage.setItem(KEY, pref);
    } catch {
      // Ignorar: el tema solo no se recordará.
    }
    if (inTauri) getCurrentWindow().setTheme(pref === "system" ? null : pref).catch(() => {});
  }, [pref, resolved]);

  return [pref, setPref, resolved];
}
