// Verifies (or fixes, with --write) that the Cargo workspace version matches
// package.json, the single source of truth. release-please updates both files
// together in its release PR (see Cargo.toml's `x-release-please-version` marker);
// this script is a local guard for manual edits and a CI sanity check.
import { readFileSync, writeFileSync } from "node:fs";

const write = process.argv.includes("--write");

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const version = pkg.version;
if (!/^\d+\.\d+\.\d+/.test(version)) {
  console.error(`package.json version "${version}" is not a valid semver.`);
  process.exit(1);
}

const cargoPath = "Cargo.toml";
const cargo = readFileSync(cargoPath, "utf8");
const match = cargo.match(/\[workspace\.package\][\s\S]*?\nversion = "([^"]+)"/);
if (!match) {
  console.error("Couldn't find [workspace.package] version in Cargo.toml.");
  process.exit(1);
}
const cargoVersion = match[1];

if (cargoVersion === version) {
  console.log(`Versions in sync: ${version}`);
  process.exit(0);
}

if (!write) {
  console.error(
    `Version mismatch: package.json=${version} Cargo.toml=${cargoVersion}. Run with --write to fix.`,
  );
  process.exit(1);
}

const updated = cargo.replace(
  /(\[workspace\.package\][\s\S]*?\nversion = ")[^"]+(")/,
  `$1${version}$2`,
);
writeFileSync(cargoPath, updated);
console.log(`Synced Cargo workspace version -> ${version}`);
