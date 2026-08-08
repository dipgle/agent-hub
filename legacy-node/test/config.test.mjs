import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { isAbsolute, join } from "node:path";
import { ALWAYS_HUMAN_ACTIONS, loadConfig, secretFromEnv, validateConfig, CONFIG_DEFAULTS } from "../src/config.mjs";

function withConfigFile(obj, fn) {
  const dir = mkdtempSync(join(tmpdir(), "hub-cfg-"));
  const file = join(dir, "hub.config.json");
  writeFileSync(file, JSON.stringify(obj));
  try {
    return fn(file);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("defaults are safe: draft-only, act stage off", () => {
  assert.equal(CONFIG_DEFAULTS.autonomy.default, "L0");
  assert.equal(CONFIG_DEFAULTS.act.enabled, false);
});

test("deploy and merge can never be auto-executed", () => {
  for (const a of ["deploy", "merge", "force_push", "delete_data", "rotate_secret"]) {
    assert.ok(ALWAYS_HUMAN_ACTIONS.has(a), `${a} must be human-only`);
  }
});

test("config file overrides merge deeply and paths become absolute", () => {
  withConfigFile({ adapters: { github: { repos: ["dipgle/tfl5"] } }, autonomy: { default: "L1" } }, (file) => {
    const cfg = loadConfig(file);
    assert.equal(cfg.autonomy.default, "L1");
    assert.deepEqual(cfg.adapters.github.repos, ["dipgle/tfl5"]);
    // untouched sibling keys survive the merge
    assert.equal(cfg.adapters.github.enabled, true);
    assert.equal(cfg.adapters.devlog.enabled, true);
    assert.ok(isAbsolute(cfg.db) && isAbsolute(cfg.log_file));
  });
});

test("invalid tier / interval / confidence are rejected loudly", () => {
  assert.throws(() => validateConfig({ ...CONFIG_DEFAULTS, autonomy: { default: "L9", projects: {} } }), /autonomy.default/);
  assert.throws(() => validateConfig({ ...CONFIG_DEFAULTS, autonomy: { default: "L0", projects: { x: "L5" } } }), /autonomy.projects.x/);
  assert.throws(() => validateConfig({ ...CONFIG_DEFAULTS, poll_interval_sec: 1 }), /poll_interval_sec/);
  assert.throws(
    () => validateConfig({ ...CONFIG_DEFAULTS, triage: { ...CONFIG_DEFAULTS.triage, min_confidence_auto: 2 } }),
    /min_confidence_auto/,
  );
  assert.throws(() => validateConfig({ ...CONFIG_DEFAULTS, routing: [{ project: "x" }] }), /routing rule/);
});

test("a malformed config file fails fast instead of running with defaults", () => {
  const dir = mkdtempSync(join(tmpdir(), "hub-cfg-bad-"));
  const file = join(dir, "hub.config.json");
  writeFileSync(file, "{ not json");
  try {
    assert.throws(() => loadConfig(file), /cannot parse config/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("secrets come from the environment, never the config file", () => {
  delete process.env.HUB_TEST_SECRET;
  assert.equal(secretFromEnv("HUB_TEST_SECRET"), null);
  process.env.HUB_TEST_SECRET = "  abc  ";
  assert.equal(secretFromEnv("HUB_TEST_SECRET"), "abc");
  process.env.HUB_TEST_SECRET = "   ";
  assert.equal(secretFromEnv("HUB_TEST_SECRET"), null, "blank must count as absent so the adapter skips");
  delete process.env.HUB_TEST_SECRET;

  const raw = JSON.stringify(loadConfig());
  assert.ok(!/sk-|gho_|bot\d+:/.test(raw), "no credential-looking literal may appear in the loaded config");
});
