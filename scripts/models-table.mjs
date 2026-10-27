// Rewrites the "Tested on" table in README.md from recipes/catalog.json.
// Run after changing recipes: npm run models-table
import { readFileSync, writeFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const catalog = JSON.parse(readFileSync(new URL("recipes/catalog.json", root), "utf8"));
const readmeUrl = new URL("README.md", root);
const readme = readFileSync(readmeUrl, "utf8");

const START = "<!-- models:start -->";
const END = "<!-- models:end -->";
const a = readme.indexOf(START);
const b = readme.indexOf(END);
if (a < 0 || b < a) throw new Error(`README.md needs ${START} and ${END}`);

const hardware = catalog.hardware;
const families = Object.fromEntries(catalog.families.map((f) => [f.id, f.name]));

// Short column names: the full names live in the workstation table above.
const short = (h) => h.name.replace(/^Dell /, "").replace(/^Pro Max with /, "");
const header = ["Model", ...hardware.map((h) => (h.available === false ? `${short(h)} (not tested yet)` : short(h)))];

const cell = (r) => {
  if (!r) return "—";
  const tps = r.tokens_per_sec ? ` ${r.tokens_per_sec} tok/s` : "";
  return `✓${tps}${r.tuned ? " · Miguel Tuned" : ""}`;
};

const rows = catalog.models.map((m) => [
  `${families[m.family] ?? m.family} · **${m.name}**${m.name.endsWith(m.size) ? "" : ` ${m.size}`}`,
  ...hardware.map((h) => cell(m.recipes[h.id])),
]);

const table = [header, header.map(() => "---"), ...rows].map((r) => `| ${r.join(" | ")} |`).join("\n");
const next = `${readme.slice(0, a + START.length)}\n${table}\n${readme.slice(b)}`;

if (next !== readme) {
  writeFileSync(readmeUrl, next);
  console.log("README.md: models table updated");
} else {
  console.log("README.md: models table already up to date");
}
