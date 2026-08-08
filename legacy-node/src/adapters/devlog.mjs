// Project-progress ingest: every project already writes a devlog
// (`<project>/logs/devlog.sqlite`, table `events`). We tail it read-only and
// turn attention-worthy events (warning / blocker / bug / test_fail /
// question) into hub messages, so a project shouting into its own log reaches
// the same triage brain as an email or a GitHub comment.
//
// Source of truth for the schema: init-project/mcp/project-agent-rs.

import { DatabaseSync } from "node:sqlite";
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { errFields, log } from "../log.mjs";

export const name = "devlog";

const NOT_PROJECTS = new Set(["AI", "scripts", "logs", "memory", "docs-map", "test-results", "crates", "sdk", "app-source"]);

/**
 * Projects live either under AI/ or straight in the workspace root (dwork,
 * social, uiux, video…), so both are searched.
 */
function devlogPath(root, project) {
  for (const base of [join(root, "AI", project), join(root, project)]) {
    const p = join(base, "logs", "devlog.sqlite");
    if (existsSync(p)) return p;
  }
  return join(root, "AI", project, "logs", "devlog.sqlite");
}

function dirsWithDevlog(base, root) {
  if (!existsSync(base)) return [];
  return readdirSync(base, { withFileTypes: true })
    .filter((d) => d.isDirectory() && !d.name.startsWith("_") && !d.name.startsWith(".") && !NOT_PROJECTS.has(d.name))
    .map((d) => d.name)
    .filter((p) => existsSync(join(base, p, "logs", "devlog.sqlite")));
}

/** Every project with a devlog, from AI/ and from the workspace root. */
export function discoverProjects(root) {
  return [...new Set([...dirsWithDevlog(join(root, "AI"), root), ...dirsWithDevlog(root, root)])].sort();
}

function maxEventId(dbPath) {
  const db = new DatabaseSync(dbPath, { readOnly: true });
  try {
    return Number(db.prepare("SELECT COALESCE(MAX(id), 0) AS m FROM events").get().m);
  } finally {
    db.close();
  }
}

function readEvents(dbPath, kinds, afterId, limit) {
  // Read-only: the hub must never write into a project's devlog by accident.
  const db = new DatabaseSync(dbPath, { readOnly: true });
  try {
    const placeholders = kinds.map(() => "?").join(", ");
    const rows = db
      .prepare(
        `SELECT id, ts, kind, actor, ref_type, ref_id, content
           FROM events
          WHERE id > ? AND kind IN (${placeholders})
          ORDER BY id ASC
          LIMIT ?`,
      )
      .all(afterId, ...kinds, limit);
    return rows;
  } finally {
    db.close();
  }
}

export async function poll({ cfg, cursors, workspaceRoot }) {
  const projects = (cfg.projects ?? []).length ? cfg.projects : discoverProjects(workspaceRoot);
  const kinds = cfg.kinds ?? ["warning", "blocker", "bug", "test_fail", "question"];
  const limit = cfg.max_per_project ?? 20;

  const messages = [];
  const nextCursors = {};
  const failures = [];
  const notInitialized = [];
  let healthy = 0;

  for (const project of projects) {
    const path = devlogPath(workspaceRoot, project);
    if (!existsSync(path)) continue;
    const key = `devlog:${project}:last_id`;

    // First sight of a project: take the current tip as the baseline instead of
    // replaying years of history (set `backfill: true` to ingest the backlog).
    if (cursors[key] === undefined && !cfg.backfill) {
      try {
        const tip = maxEventId(path);
        nextCursors[key] = String(tip);
        log.info("devlog_baseline_set", { project, last_id: tip });
        healthy += 1;
      } catch (e) {
        if (isUninitialized(e)) {
          // The file exists but the project never logged anything yet — normal,
          // not a fault. Recorded at info so it is still visible.
          log.info("devlog_not_initialized", { project, path });
          notInitialized.push(project);
        } else {
          log.error("devlog_baseline_failed", { project, path, ...errFields(e) });
          failures.push(`${project} (baseline): ${e.message}`);
        }
      }
      continue;
    }

    const afterId = Number(cursors[key] ?? 0);

    let rows;
    try {
      rows = readEvents(path, kinds, afterId, limit);
      healthy += 1;
    } catch (e) {
      if (isUninitialized(e)) {
        log.info("devlog_not_initialized", { project, path });
        notInitialized.push(project);
        continue;
      }
      // A locked/corrupt project DB is a real condition — surface it.
      log.error("devlog_read_failed", { project, path, ...errFields(e) });
      failures.push(`${project}: ${e.message}`);
      continue;
    }

    for (const r of rows) {
      messages.push({
        source: name,
        external_id: `${project}:${r.id}`,
        thread_key: `devlog:${project}:${r.ref_type ?? "-"}:${r.ref_id ?? "-"}`,
        project,
        sender: r.actor ? `devlog:${r.actor}` : `devlog:${project}`,
        sender_trust: "trusted", // our own machine wrote this
        subject: `[${project}] ${r.kind}${r.ref_id ? ` ${r.ref_id}` : ""}`,
        body: r.content ?? "",
        url: null,
        received_at: r.ts,
        raw: { stream: "devlog_events", project, event_id: r.id, kind: r.kind, ref_type: r.ref_type, ref_id: r.ref_id },
      });
    }

    if (rows.length) nextCursors[key] = String(rows.at(-1).id);
  }

  // Only a total wipeout is an adapter failure; otherwise report the partial so
  // the cursors earned by the healthy projects still get committed.
  if (failures.length && healthy === 0) throw new Error(`devlog poll failed: ${failures.join("; ")}`);
  const notes = [
    failures.length ? `failed: ${failures.join("; ")}` : null,
    notInitialized.length ? `no events table yet: ${notInitialized.join(", ")}` : null,
  ].filter(Boolean);
  return {
    messages,
    cursors: nextCursors,
    skipped: notes.length ? notes.join(" | ") : null,
  };
}

/** A devlog file that exists but was never written to by project-agent. */
function isUninitialized(e) {
  return /no such table: events/i.test(String(e?.message ?? e));
}
