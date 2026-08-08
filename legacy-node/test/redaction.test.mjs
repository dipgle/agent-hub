import { test } from "node:test";
import assert from "node:assert/strict";
import { compileExtraPatterns, EXTERNAL_CHANNELS, leakScan } from "../src/redaction.mjs";

test("the observed real leak — a reply quoting workspace memory — is caught", () => {
  // Verbatim shape of the evidence line produced by a real triage run on 2026-07-26.
  const draft = "memory: tfl5 security hardening 2026-07 — CÒN OPEN: PG SPOF, HMAC-key rotate";
  const hits = leakScan(draft);
  assert.ok(hits.includes("internal_notes_citation"), JSON.stringify(hits));
  assert.ok(hits.includes("internal_risk_language"), JSON.stringify(hits));
});

test("hosts, IPs, local paths, wikilinks and credentials are all caught", () => {
  const cases = {
    "deploy chạy trên vps-a rồi reload": "internal_host",
    "node ở 46.250.231.130:41100": "ip_address",
    "xem /Users/hanguyen/Documents/projects/tfl5": "local_filesystem_path",
    "chi tiết ở [[tfl5-deploy-nopasswd-scope]]": "memory_wikilink",
    "dùng bearer token của admin": "credential_word",
    "token: 123456789:AAHfSjKLmnOPqrstuvwxyz0123456789abc": "credential_literal",
    "-----BEGIN RSA PRIVATE KEY-----": "private_key_block",
  };
  for (const [text, label] of Object.entries(cases)) {
    assert.ok(leakScan(text).includes(label), `${label} missed in: ${text}`);
  }
});

test("an ordinary customer-facing reply passes clean", () => {
  const clean = [
    "Cảm ơn bạn đã báo. Mình đã ghi nhận lỗi ở trang đăng nhập và sẽ kiểm tra trong hôm nay.",
    "Tính năng xuất Excel dự kiến có trong bản tới. Mình sẽ thông báo khi xong.",
    "Bạn thử đăng xuất rồi đăng nhập lại giúp mình nhé, nếu vẫn lỗi mình sẽ kiểm tra tiếp.",
  ];
  for (const t of clean) assert.deepEqual(leakScan(t), [], t);
});

test("only channels that leave the machine are gated", () => {
  assert.ok(EXTERNAL_CHANNELS.has("github"));
  assert.ok(EXTERNAL_CHANNELS.has("email"));
  assert.ok(EXTERNAL_CHANNELS.has("telegram"));
  assert.ok(!EXTERNAL_CHANNELS.has("notify"), "the local brief is allowed to contain internal detail");
  assert.ok(!EXTERNAL_CHANNELS.has("devlog"));
});

test("config patterns compile, and a broken one is reported not swallowed", () => {
  const errors = [];
  const compiled = compileExtraPatterns(["tafalo-internal", "([unclosed"], (src, e) => errors.push(src));
  assert.equal(compiled.length, 1);
  assert.equal(errors.length, 1);
  assert.ok(leakScan("dự án tafalo-internal", compiled).length > 0);
});
