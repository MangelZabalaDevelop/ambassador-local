// Writes the updater's latest.json from the release's signatures, once every build is done.
// The three builds finish within seconds of each other, and each one rewriting latest.json
// can drop another's platforms (v0.1.7 lost linux-aarch64 that way).
// Usage: node scripts/updater-json.mjs <folder with the .sig files> <owner/repo> <tag> [release notes file]
// The notes are shown in the app's update box.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [dir, repo, tag, notesFile] = process.argv.slice(2);
if (!dir || !repo || !tag) throw new Error("usage: updater-json.mjs <dir> <owner/repo> <tag> [notes file]");

// File ending → the updater's platform keys (the plain key is the default package for that system).
const kinds = [
  ["_amd64.AppImage", ["linux-x86_64", "linux-x86_64-appimage"]],
  ["_amd64.deb", ["linux-x86_64-deb"]],
  ["_aarch64.AppImage", ["linux-aarch64", "linux-aarch64-appimage"]],
  ["_arm64.deb", ["linux-aarch64-deb"]],
  ["_x64-setup.exe", ["windows-x86_64", "windows-x86_64-nsis"]],
];

const platforms = {};
for (const sig of readdirSync(dir).filter((f) => f.endsWith(".sig"))) {
  const name = sig.slice(0, -4);
  const kind = kinds.find(([end]) => name.endsWith(end));
  if (!kind) continue;
  const entry = { signature: readFileSync(join(dir, sig), "utf8"), url: `https://github.com/${repo}/releases/download/${tag}/${name}` };
  for (const key of kind[1]) platforms[key] = entry;
}

const missing = kinds.flatMap(([, keys]) => keys).filter((k) => !platforms[k]);
if (missing.length) throw new Error(`no signed package for: ${missing.join(", ")}`);

const version = tag.replace(/^v/, "");
const notes = notesFile ? readFileSync(notesFile, "utf8").trim() : "";
const out = { version, notes, pub_date: new Date().toISOString(), platforms };
writeFileSync(join(dir, "latest.json"), JSON.stringify(out, null, 2) + "\n");
console.log(`latest.json ${version}: ${Object.keys(platforms).join(" ")}`);
