import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

test("the release stays private until its exact artifact can be verified", async () => {
  const config = JSON.parse(await readFile("release-please-config.json", "utf8"));
  const workflow = await readFile(".github/workflows/release.yml", "utf8");
  assert.equal(config.draft, true, "Release Please must create a draft");
  assert.equal(config["force-tag-creation"], true, "the draft must retain a source tag");
  const guard = workflow.indexOf("- name: Assert draft release and exact source tag");
  const upload = workflow.indexOf("- uses: tauri-apps/tauri-action@");
  assert.ok(guard >= 0 && upload > guard, "source and draft checks must precede upload");
  assert.match(workflow, /releaseDraft:\s*true/, "Tauri must upload to the existing draft");
  assert.doesNotMatch(workflow, /--draft=false/, "the workflow cannot publish before verification");
});
