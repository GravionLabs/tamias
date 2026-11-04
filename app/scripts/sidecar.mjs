// Builds the tamias CLI and places it where `bundle.externalBin` (src-tauri/tauri.conf.json)
// expects it: src-tauri/binaries/tamias-<target-triple>[.exe], on Windows also tamiasw.
//
// Tauri needs these files for `tauri dev` and `tauri build`, and its build script needs them for
// every `cargo build|clippy|test -p tamias-app`, so run this once before working on the app and
// again after changing the CLI:
//
//   pnpm sidecar                                  # debug build for the host
//   pnpm sidecar --release --target <triple>      # what the release job bundles
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: { release: { type: "boolean", default: false }, target: { type: "string" } },
});

const app = join(dirname(fileURLToPath(import.meta.url)), "..");
const root = join(app, "..");
const host = /^host: (.+)$/m.exec(execFileSync("rustc", ["-vV"], { encoding: "utf8" }))?.[1];
const target = values.target ?? host;
if (!target) throw new Error("cannot determine the host target triple from `rustc -vV`");

const cargo = ["build", "-p", "tamias", "--bins", "--locked"];
if (values.release) cargo.push("--release");
if (values.target) cargo.push("--target", values.target);
execFileSync("cargo", cargo, { cwd: root, stdio: "inherit" });

const targetDir = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const built = join(targetDir, values.target ?? "", values.release ? "release" : "debug");
const windows = target.includes("-windows-");
const exe = windows ? ".exe" : "";
const binaries = join(app, "src-tauri", "binaries");
mkdirSync(binaries, { recursive: true });
for (const name of windows ? ["tamias", "tamiasw"] : ["tamias"]) {
  const to = join(binaries, `${name}-${target}${exe}`);
  copyFileSync(join(built, `${name}${exe}`), to);
  console.log(`sidecar: ${to}`);
}
