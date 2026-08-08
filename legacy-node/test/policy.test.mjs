import { test } from "node:test";
import assert from "node:assert/strict";
import {
  decideOutcome, effectiveTier, emailAddress, githubReplyTarget, humanOnlyActions,
  resolveProject, resolveTrust,
} from "../src/policy.mjs";

const cfg = {
  autonomy: { default: "L1", projects: { tfl5: "L2", sdvi: "L0" } },
  triage: { min_confidence_auto: 0.8 },
  trust: { github_logins: ["dipgle"], emails: ["owner@dipgle.com"], telegram_chat_ids: ["12345"], trusted_sources: ["devlog", "cli"] },
  routing: [
    { when: { repo: "dipgle/tcc-node" }, project: "tcc" },
    { when: { source: "email", subject_contains: "sdvi" }, project: "sdvi" },
  ],
};

const baseDecision = {
  kind: "question",
  severity: "p2",
  project: "tfl5",
  summary: "s",
  reply_draft: "xin chào",
  proposed_actions: [{ type: "reply", detail: "answer" }],
  evidence: [],
  needs_human: false,
  confidence: 0.9,
};

test("routing: explicit rule wins over heuristics", () => {
  const msg = { source: "github", subject: "[x] y", raw: { repo: "dipgle/tcc-node" } };
  assert.equal(resolveProject(msg, cfg, ["tcc-node", "tcc"]), "tcc");
});

test("routing: repo name falls back to a same-named project folder", () => {
  const msg = { source: "github", subject: "CI failed", raw: { repo: "dipgle/tfl5" } };
  assert.equal(resolveProject(msg, cfg, ["tfl5"]), "tfl5");
});

test("routing: subject tag [project] is honoured only for known projects", () => {
  assert.equal(resolveProject({ source: "email", subject: "[mailler] bug" }, cfg, ["mailler"]), "mailler");
  assert.equal(resolveProject({ source: "email", subject: "[nope] bug" }, cfg, ["mailler"]), null);
});

test("routing: multi-key rule requires every key to match", () => {
  assert.equal(resolveProject({ source: "email", subject: "về sdvi" }, cfg, []), "sdvi");
  assert.equal(resolveProject({ source: "github", subject: "về sdvi" }, cfg, []), null);
});

test("trust: known github login / email / chat id are trusted, others not", () => {
  assert.equal(resolveTrust({ source: "github", sender: "dipgle" }, cfg), "trusted");
  assert.equal(resolveTrust({ source: "github", sender: "stranger" }, cfg), "untrusted");
  assert.equal(resolveTrust({ source: "email", sender: "Owner <OWNER@dipgle.com>" }, cfg), "trusted");
  assert.equal(resolveTrust({ source: "email", sender: "x@evil.com" }, cfg), "untrusted");
  assert.equal(resolveTrust({ source: "telegram", sender: "tg", raw: { chat_id: "12345" } }, cfg), "trusted");
  assert.equal(resolveTrust({ source: "devlog", sender: "devlog:tfl5" }, cfg), "trusted");
});

test("tier: untrusted sender is capped at L0 even on an L2 project", () => {
  assert.equal(effectiveTier("tfl5", "trusted", cfg), "L2");
  assert.equal(effectiveTier("tfl5", "untrusted", cfg), "L0");
  assert.equal(effectiveTier("sdvi", "trusted", cfg), "L0");
  assert.equal(effectiveTier(null, "trusted", cfg), "L1");
});

test("github reply target from notification detail, issue_url, or html url", () => {
  assert.equal(githubReplyTarget({}, { repo: "dipgle/tfl5", detail: { number: 42 } }), "dipgle/tfl5#42");
  assert.equal(githubReplyTarget({}, { repo: "dipgle/tfl5", issue_url: "https://api.github.com/repos/dipgle/tfl5/issues/7" }), "dipgle/tfl5#7");
  assert.equal(githubReplyTarget({ url: "https://github.com/dipgle/tfl5/pull/9" }, { repo: "dipgle/tfl5" }), "dipgle/tfl5#9");
  assert.equal(githubReplyTarget({}, { repo: "dipgle/tfl5" }), null);
});

test("email address extraction", () => {
  assert.equal(emailAddress("Nguyen A <a@b.com>"), "a@b.com");
  assert.equal(emailAddress("A@B.com"), "a@b.com");
  assert.equal(emailAddress(null), null);
});

test("auto_reply only when tier>=L1, trusted, confident, repliable kind", () => {
  const msg = { source: "github", sender: "dipgle", raw: { repo: "dipgle/tfl5", detail: { number: 5 } } };
  const o = decideOutcome({ msg, decision: baseDecision, tier: "L1", trust: "trusted", cfg });
  assert.equal(o.action, "auto_reply");
  assert.equal(o.target, "dipgle/tfl5#5");
});

test("L0 never auto-sends", () => {
  const msg = { source: "github", sender: "dipgle", raw: { repo: "dipgle/tfl5", detail: { number: 5 } } };
  const o = decideOutcome({ msg, decision: baseDecision, tier: "L0", trust: "untrusted", cfg });
  assert.equal(o.action, "await_human");
  assert.match(o.reason, /drafts only/);
});

test("low confidence, needs_human, security kind and bug kind all route to a human", () => {
  const msg = { source: "github", sender: "dipgle", raw: { repo: "dipgle/tfl5", detail: { number: 5 } } };
  for (const [patch, re] of [
    [{ confidence: 0.5 }, /confidence/],
    [{ needs_human: true }, /needs_human/],
    [{ kind: "security" }, /security/],
    [{ kind: "bug" }, /not auto-repliable/],
    [{ reply_draft: "  " }, /empty reply_draft/],
  ]) {
    const o = decideOutcome({ msg, decision: { ...baseDecision, ...patch }, tier: "L2", trust: "trusted", cfg });
    assert.equal(o.action, "await_human", JSON.stringify(patch));
    assert.match(o.reason, re);
  }
});

test("a tripwire hit outranks a confident, trusted, auto-repliable decision", () => {
  const msg = { source: "github", sender: "dipgle", raw: { repo: "dipgle/tfl5", detail: { number: 5 } } };
  const o = decideOutcome({ msg, decision: baseDecision, tier: "L2", trust: "trusted", tripwire: ["role_override"], cfg });
  assert.equal(o.action, "await_human");
  assert.match(o.reason, /tripwire/);
});

test("deploy/merge style actions always need a human", () => {
  const decision = { ...baseDecision, proposed_actions: [{ type: "reply", detail: "x" }, { type: "deploy", detail: "ship it" }] };
  assert.deepEqual(humanOnlyActions(decision), ["deploy"]);
  const msg = { source: "github", sender: "dipgle", raw: { repo: "dipgle/tfl5", detail: { number: 5 } } };
  const o = decideOutcome({ msg, decision, tier: "L2", trust: "trusted", cfg });
  assert.equal(o.action, "await_human");
  assert.match(o.reason, /requires human/);
});

test("spam/noise is ignored without bothering anyone", () => {
  const msg = { source: "email", sender: "x@evil.com" };
  const o = decideOutcome({ msg, decision: { ...baseDecision, kind: "spam" }, tier: "L0", trust: "untrusted", cfg });
  assert.equal(o.action, "ignore");
});

test("a cli question is answered back through the local notify channel", () => {
  const msg = { source: "cli", sender: "cli:owner", raw: { stream: "cli" } };
  const o = decideOutcome({ msg, decision: baseDecision, tier: "L1", trust: "trusted", cfg });
  assert.equal(o.action, "auto_reply");
  assert.equal(o.channel, "notify");
  assert.equal(o.target, "local");
});

test("no reply target means human review, not a lost reply", () => {
  const msg = { source: "devlog", sender: "devlog:tfl5", raw: { project: "tfl5" } };
  const o = decideOutcome({ msg, decision: baseDecision, tier: "L2", trust: "trusted", cfg });
  assert.equal(o.action, "await_human");
  assert.match(o.reason, /no reply target/);
});
