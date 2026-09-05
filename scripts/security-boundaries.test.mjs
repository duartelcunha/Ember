import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

test("every registered application command participates in the Tauri ACL manifest", async () => {
  const [shell, build] = await Promise.all([
    readFile("src-tauri/src/lib.rs", "utf8"), readFile("src-tauri/build.rs", "utf8"),
  ]);
  const handler = shell.match(/generate_handler!\[([\s\S]*?)\]\)/)?.[1];
  assert.ok(handler, "the command registry must be inspectable");
  const registered = handler.split(",").map(s => s.trim().split("::").at(-1)).filter(Boolean).sort();
  const declared = [...build.matchAll(/"([a-z_]+)"/g)].map(m => m[1]).sort();
  assert.deepEqual(registered, declared, "an unlisted command could bypass the application permission contract");
});

test("floating surfaces cannot receive settings, credential or filesystem commands", async () => {
  for (const [label, allowed] of Object.entries({
    overlay: ["allow-floating-position", "allow-overlay-snapshot"],
    picker: ["allow-floating-position", "allow-picker-snapshot"],
    animations: ["allow-close-splash", "allow-finalize-quit"],
  })) {
    const capability = JSON.parse(await readFile(`src-tauri/capabilities/${label}.json`, "utf8"));
    assert.equal(capability.remote, undefined);
    assert.deepEqual(capability.permissions.filter(p => p !== "core:default").sort(), allowed.sort());
    assert.ok(!capability.windows.includes("*"));
  }
});
