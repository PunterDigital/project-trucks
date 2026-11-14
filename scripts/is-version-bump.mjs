// Prints "true" when the diff between two commits is only a release bump:
// version lines in package.json / tauri.conf.json / Cargo.toml, the app's own
// entry in Cargo.lock, and CHANGELOG.md. CI uses it to skip the ~13 minute
// Windows job on `chore(release)` PRs: the tag's release workflow builds and
// checks the installers anyway. Anything else (or any doubt) prints "false".
//
// Usage: node scripts/is-version-bump.mjs <base-sha> <head-sha>
import { execFileSync } from "node:child_process";

const [base, head] = process.argv.slice(2);

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8" });
}

function versionOnly() {
  if (!base || !head || /^0+$/.test(base)) return false;
  // Three dots: diff from the merge base, so commits that landed on the base
  // branch after the PR was opened don't count as part of it.
  const range = `${base}...${head}`;
  const files = git("diff", "--name-only", range).split("\n").filter(Boolean);
  if (files.length === 0) return false;

  const versionLine = {
    "package.json": /^\s*"version": "[^"]+",?$/,
    "src-tauri/tauri.conf.json": /^\s*"version": "[^"]+",?$/,
    "src-tauri/Cargo.toml": /^version = "[^"]+"$/,
    "src-tauri/Cargo.lock": /^version = "[^"]+"$/,
  };

  for (const file of files) {
    if (file === "CHANGELOG.md") continue;
    const allowed = versionLine[file];
    if (!allowed) return false;
    const diff = git("diff", "--unified=1", range, "--", file);
    const changed = diff
      .split("\n")
      .filter((l) => /^[+-]/.test(l) && !/^(\+\+\+|---) /.test(l))
      .map((l) => l.slice(1));
    if (changed.length === 0 || !changed.every((l) => allowed.test(l))) return false;
    // Cargo.lock: only the app's own package may move, never a dependency.
    if (file === "src-tauri/Cargo.lock") {
      const hunks = diff.split("\n@@").slice(1);
      if (!hunks.every((h) => h.includes('name = "projecttrucks"'))) return false;
    }
  }
  return true;
}

let result = false;
try {
  result = versionOnly();
} catch (err) {
  console.error(err.message);
}
console.log(result ? "true" : "false");
