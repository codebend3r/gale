# Feature tests

End-to-end tests for the features tracked in [`docs/features-table.md`](../../docs/features-table.md).
Each test drives the real `gale` binary inside a throwaway project directory
and checks the output the way a user or a CI script would.

## Running

```sh
cargo build --release      # the suite runs target/release/gale
bun test tests/features    # or: npm run test:features
```

Set `GALE_BIN=/path/to/gale` to test a different binary.

## Conventions

- One file per feature area.
- A test written as `test.failing(...)` documents behaviour Gale does not have
  yet. It passes while the feature is missing and fails as soon as the feature
  lands, at which point the marker is removed. This keeps the suite green
  while still pinning down the exact expected behaviour up front.
- A plain `test(...)` next to failing ones is an anchor: it pins the behaviour
  that already works so a change cannot break it while adding the new part.
- Expected strings come from Stylelint v17's source, not from memory. When a
  message is quoted verbatim it is the text Stylelint emits.
- `helpers.ts` owns process spawning, temp projects, and the JSON result
  types. `lsp-client.ts` is a minimal JSON-RPC client for `gale --lsp`.
