# Efficient project memory

Use Engram selectively for durable project knowledge. Treat memory context already
supplied by the lifecycle hook as the initial orientation; do not immediately
fetch the same context again unless required by compaction recovery or the task.

For ordinary retrieval, use `mem_context` with `compact: true`, `max_bytes: 4096`,
and `older_history: false`. Use focused `mem_search` queries with `limit: 5` and
`response_format: "compact"`, scoped to the relevant project when known. Expand
the budget or search scope when evidence is insufficient; omitted results are
not evidence that no memory exists. Fetch full observations by ID before relying
on their details or updating them. Reuse observations already read in this turn.

Preserve verified decisions, working commands, non-obvious fixes, constraints,
and useful handoffs. Update an existing topic after searching and reading it;
retain valid facts and mark superseded facts as history. Avoid routine progress,
raw logs, duplicate summaries, and speculative memories. Never delete project
memory to save tokens. Keep lifecycle registration, session attribution,
conflict handling, and compaction recovery intact.

## Project orientation

- Engram identifies this repository as `infamous-us`. Keep its project memory
  separate from upstream or European-release projects.
- This is the inFamous US (`BCUS98119`) Windows recompilation project. The installer
  also accepts European `BCES00220` dumps; preserve the region-specific behavior.
- Start with `README.md`, `installer/README.md`, and
  `installer/regions/BCUS98119/README.md`. Check claims against current code and
  manifests before recording them as verified knowledge.
- `installer/infamous.toml` defines the generation pipeline, generated-file rules,
  build commands, and export settings. Do not commit game dumps or generated game
  code/data. Preserve the hand-written exceptions listed in `[generated].exclude`
  and `.gitignore`.
- US locks and PPU patches live under `installer/regions/BCUS98119/`; patches
  absent there fall back to `installer/patches/`. After changing the European PPU
  patches, run `python installer/regions/BCUS98119/tools/check_sync.py .` from the
  repository root and follow the US README if rebasing is needed.
- For installer changes, the Windows CI checks are `cargo fmt --check` and
  `cargo build --release --locked`, run from `installer/`. Full game validation
  additionally requires the user's dump and the documented native toolchain.
- Distinguish source-verified behavior and documented commands from commands or
  gameplay actually validated in the current environment.
