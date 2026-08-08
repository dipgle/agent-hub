# legacy-node — the Node prototype (archived 2026-07-26)

This is the original zero-dependency Node implementation of hub. It proved the
loop end-to-end on real data (30 GitHub notifications ingested, 9 real triage
decisions, coalescing, outbox delivery, auto-reply at L1) and then served as the
**oracle** for the Rust port.

It is kept only for reference/diffing. The Rust crate in `../rust/` is canonical:
same sqlite schema, same `hub.config.json`, same CLI surface, 50 tests green.

## Running it (if you ever need the oracle back)

```bash
cd legacy-node
node --no-warnings=ExperimentalWarning --test test/*.test.mjs      # 47 tests
HUB_CONFIG=../hub.config.json node bin/hub.mjs doctor
```

Note: without `HUB_CONFIG` it now resolves its paths relative to THIS directory
(`legacy-node/data/hub.sqlite`), not the live database — deliberate, so an
accidental run cannot touch real state.

Delete whenever you are done comparing:

```bash
rm -rf legacy-node
```
