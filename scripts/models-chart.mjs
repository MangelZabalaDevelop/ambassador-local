// Draws the "Which model should I pick?" charts in docs/img/ from recipes/catalog.json:
// one per workstation, in light and dark, with the models grouped by intelligence and
// a bar for the speed measured on that workstation. Run with `npm run models-table`.
import { mkdirSync, writeFileSync } from "node:fs";
import { readFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const catalog = JSON.parse(readFileSync(new URL("recipes/catalog.json", root), "utf8"));
const out = new URL("docs/img/", root);
mkdirSync(out, { recursive: true });

const families = Object.fromEntries(catalog.families.map((f) => [f.id, f.name]));

// Colors checked with the dataviz palette validator (CVD-safe, ≥3:1 on GitHub's light and dark pages).
const themes = {
  light: { surface: "#ffffff", border: "#d1d9e0", band: "#f6f8fa", ink: "#1f2328", soft: "#59636e", faint: "#818b98", bar: "#0672cb", tuned: "#4e8a00" },
  dark: { surface: "#0d1117", border: "#3d444d", band: "#151b23", ink: "#f0f6fc", soft: "#9198a1", faint: "#6e7681", bar: "#3d8fe0", tuned: "#5f9e00" },
};

const tiers = [
  { level: 5, title: "Smartest", hint: "Intelligence Index 25 or more: best answers, hardest tasks" },
  { level: 4, title: "Very smart", hint: "Intelligence Index 10 to 24: a strong balance" },
  { level: 3, title: "Smart", hint: "Intelligence Index under 10: quick everyday help" },
];

const W = 900;
const PAD = 28;
const NAME_W = 300; // model name and what it is good at
const VALUE_W = 150; // "116.9 tokens/s" after the bar
const BAR_X = PAD + NAME_W;
const BAR_MAX = W - PAD - VALUE_W - BAR_X;
const ROW = 44;
const TIER_HEAD = 40;

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const short = (h) => h.name.replace(/^Dell /, "");
const fmt = (n) => (Number.isInteger(n) ? String(n) : n.toFixed(1));

function chart(hw, t) {
  // Which of the workstation's systems offer this recipe, said only when it is not all of them.
  const systems = (r) => (r.vllm || r.tensorfold ? ["linux"] : r.os ?? hw.os);
  const only = (r) => {
    const s = systems(r);
    if (s.length >= hw.os.length) return null;
    return s[0] === "windows" ? "Windows only" : "Ubuntu only";
  };
  const models = catalog.models
    .filter((m) => m.recipes[hw.id])
    .map((m) => ({ m, r: m.recipes[hw.id] }));
  const max = Math.max(...models.map((x) => x.r.tokens_per_sec ?? 0));
  const groups = tiers
    .map((tier) => ({
      ...tier,
      rows: models
        .filter((x) => x.m.intelligence === tier.level)
        .sort((a, b) => (b.r.tokens_per_sec ?? 0) - (a.r.tokens_per_sec ?? 0)),
    }))
    .filter((g) => g.rows.length);

  const top = 112;
  const legendH = 66;
  const height = top + groups.reduce((h, g) => h + TIER_HEAD + g.rows.length * ROW + 10, 0) + legendH + PAD;
  const font = `font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"`;
  const s = [];
  s.push(`<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${height}" viewBox="0 0 ${W} ${height}" ${font} role="img" aria-labelledby="t d">`);
  s.push(`<title id="t">Which model should I pick on the ${esc(short(hw))}?</title>`);
  s.push(`<desc id="d">Models grouped by the Artificial Analysis Intelligence Index, each with the speed measured on the ${esc(short(hw))}. ${esc(
    models.map((x) => `${x.m.name}: ${x.m.aa_index != null ? "Intelligence Index " + x.m.aa_index : "Intelligence Index estimated"}, ${x.r.tokens_per_sec ? fmt(x.r.tokens_per_sec) + " tokens per second" : "speed not measured yet"}`).join("; "),
  )}.</desc>`);
  s.push(`<rect x="0.5" y="0.5" width="${W - 1}" height="${height - 1}" rx="12" fill="${t.surface}" stroke="${t.border}"/>`);

  // Header: the workstation, then how to read the chart in one line.
  s.push(`<text x="${PAD}" y="${PAD + 18}" font-size="22" font-weight="700" fill="${t.ink}">${esc(short(hw))}</text>`);
  s.push(`<text x="${PAD}" y="${PAD + 42}" font-size="14" fill="${t.soft}">${esc(hw.gpu)} · ${hw.memory_gb} GB</text>`);
  s.push(`<text x="${PAD}" y="${PAD + 70}" font-size="14" fill="${t.ink}"><tspan font-weight="700">Higher group = smarter.</tspan> <tspan font-weight="700">Longer bar = faster answers</tspan><tspan fill="${t.soft}"> (words appear on screen faster; measured on this workstation).</tspan></text>`);

  let y = top;
  for (const g of groups) {
    // Tier band: stars and a plain-language name.
    s.push(`<rect x="${PAD - 12}" y="${y}" width="${W - 2 * PAD + 24}" height="${TIER_HEAD - 8}" rx="6" fill="${t.band}"/>`);
    const stars = "★".repeat(g.level) + "☆".repeat(5 - g.level);
    s.push(`<text x="${PAD}" y="${y + 22}" font-size="15" fill="${t.ink}"><tspan fill="#d4a017" letter-spacing="1">${stars}</tspan><tspan dx="10" font-weight="700">${g.title}</tspan><tspan dx="8" fill="${t.soft}">${g.hint}</tspan></text>`);
    y += TIER_HEAD;

    for (const { m, r } of g.rows) {
      const tuned = !!r.tuned;
      const family = families[m.family] ?? m.family;
      const size = m.name.endsWith(m.size) ? "" : ` ${m.size}`;
      const name = `${m.name}${size}`;
      // The Intelligence Index from Artificial Analysis, or "estimated" while it has not rated the model.
      const score = m.aa_index != null ? `Index ${m.aa_index}` : "Estimated";
      const uses = m.best_for.slice(0, only(r) ? 1 : 2);
      const note = [score, family, uses.join(", "), only(r)].filter(Boolean).join(" · ");
      const cy = y + ROW / 2;
      s.push(`<g>`);
      s.push(`<title>${esc(`${family} ${name}, ${m.aa_index != null ? `Intelligence Index ${m.aa_index}` : "Intelligence Index estimated"}: ${r.tokens_per_sec ? fmt(r.tokens_per_sec) + " tokens/s" : "speed not measured yet"}. Good for ${m.best_for.join(", ")}.`)}</title>`);
      s.push(`<text x="${PAD}" y="${cy - 3}" font-size="14" font-weight="600" fill="${t.ink}">${esc(fit(name, 34))}${r.recommended ? ` <tspan fill="#d4a017">♛</tspan>` : ""}</text>`);
      s.push(`<text x="${PAD}" y="${cy + 14}" font-size="12" fill="${t.soft}">${esc(fit(note, 46))}</text>`);
      if (r.tokens_per_sec) {
        const w = Math.max(6, (r.tokens_per_sec / max) * BAR_MAX);
        const h = 16;
        const by = cy - h / 2 + 2;
        const rad = 4;
        // Square at the start, rounded at the end.
        s.push(`<path d="M${BAR_X} ${by}h${w - rad}a${rad} ${rad} 0 0 1 ${rad} ${rad}v${h - 2 * rad}a${rad} ${rad} 0 0 1 -${rad} ${rad}h-${w - rad}z" fill="${tuned ? t.tuned : t.bar}"/>`);
        s.push(`<text x="${BAR_X + w + 8}" y="${cy + 7}" font-size="14" fill="${t.ink}"><tspan font-weight="700">${fmt(r.tokens_per_sec)}</tspan><tspan fill="${t.soft}"> tokens/s</tspan></text>`);
      } else {
        s.push(`<text x="${BAR_X}" y="${cy + 7}" font-size="13" font-style="italic" fill="${t.faint}">speed not measured yet</text>`);
      }
      s.push(`</g>`);
      y += ROW;
    }
    y += 10;
  }

  // Legend: what the two bar colors and the crown mean.
  const ly = y + 14;
  const hasTuned = models.some((x) => x.r.tuned);
  s.push(`<line x1="${PAD}" y1="${y}" x2="${W - PAD}" y2="${y}" stroke="${t.border}"/>`);
  s.push(`<rect x="${PAD}" y="${ly}" width="14" height="14" rx="3" fill="${t.bar}"/>`);
  s.push(`<text x="${PAD + 22}" y="${ly + 12}" font-size="13" fill="${t.soft}">Speed measured on this workstation</text>`);
  if (hasTuned) {
    const lx = PAD + 270;
    s.push(`<rect x="${lx}" y="${ly}" width="14" height="14" rx="3" fill="${t.tuned}"/>`);
    s.push(`<text x="${lx + 22}" y="${ly + 12}" font-size="13" fill="${t.soft}">Miguel Tuned: tuned and tested by hand   <tspan fill="#d4a017">♛</tspan> recommended</text>`);
  }
  s.push(`<text x="${PAD}" y="${ly + 40}" font-size="12" fill="${t.faint}">Intelligence: Artificial Analysis Intelligence Index v5 (November 2026). Models it has not rated yet are estimated from their closest rated relative.</text>`);
  s.push(`</svg>`);
  return s.join("\n") + "\n";
}

// Cuts a label that would run into the bars (they are short; this is a guard).
function fit(text, chars) {
  return text.length <= chars ? text : `${text.slice(0, chars - 1)}…`;
}

const made = [];
for (const hw of catalog.hardware.filter((h) => h.available !== false)) {
  for (const [mode, t] of Object.entries(themes)) {
    const file = `pick-${hw.id}-${mode}.svg`;
    writeFileSync(new URL(file, out), chart(hw, t));
    made.push(file);
  }
}
console.log(`docs/img: ${made.join(", ")}`);
