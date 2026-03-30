# SpaceAway Overnight Autonomous Development Session

Copy everything below this line and paste it as a new Claude Code conversation.

---

You are running an autonomous 7+ hour development session on SpaceAway, a cooperative first-person space exploration game with a custom Rust engine (wgpu + rapier3d). The user is sleeping. You must work independently, making progress across multiple phases. No human input is available until the session ends.

## Project Location
`/Users/dante/Projects/SpaceAway`

## Critical Rules

1. **Read CLAUDE.md first** — it has all conventions, architecture, build commands
2. **`cargo clippy --workspace -- -D warnings` must pass after EVERY change** — no exceptions
3. **`cargo test --workspace` must pass after EVERY change** — no exceptions
4. **Commit after each logical change** — conventional commits (`fix()`, `refactor()`, `feat()`, `perf()`, `test()`)
5. **Never break existing functionality** — the game must still compile and run
6. **Write a progress report** to `/Users/dante/Projects/SpaceAway/docs/overnight-report.md` after each phase
7. **Files must stay under 300 lines** — split when they grow (project convention)
8. **Research before implementing** — read ALL connected code before changing anything
9. **Confirm bugs with proof** — trace data flow, check values, don't guess
10. **Add tests for anything you fix** — the headless test harness works (`cargo test -p spaceaway --test full_descent_test`)

## Current State

- main.rs is 3702 lines (12x over the 300-line convention — MUST be split)
- 103 Rust source files, 27,038 total lines
- 400+ tests all passing
- Known bugs: render flickering (instance buffer timing), terrain visibility during approach, collision working in headless test but untested in-game
- Recent additions: draw call batching, puffin profiler, wgpu-profiler, profiling crate, headless descent test
- Key root cause found and fixed: chunk_dist() was comparing displaced chunk centers (77km offset) against camera position — HeightField colliders never created

## Phase 1: Refactor main.rs (HIGHEST PRIORITY)

main.rs is 3702 lines. The project convention is 300 lines per file. Split it into focused modules:

1. Read the ENTIRE main.rs to understand the structure
2. Identify logical sections (input handling, physics, terrain, rendering, UI, drive system, teleports, etc.)
3. Extract each section into its own module file under `crates/spaceaway/src/`:
   - `game_state.rs` — the App struct fields and initialization
   - `input_handler.rs` — keyboard/mouse input processing
   - `helm_mode.rs` — seated helm physics, thrust, steering
   - `walk_mode.rs` — standing/walking character controller integration
   - `drive_system.rs` — cruise/warp galactic position tracking, auto-disengage logic
   - `teleport.rs` — key 0/8/9/backquote debug teleport handlers
   - `render_frame.rs` — draw command assembly, camera sync, terrain draw integration
   - Keep main.rs as just the event loop dispatcher (~200 lines)
4. Each extracted module should be self-contained with clear input/output
5. Update imports and ensure everything compiles after each extraction
6. Run full test suite after each file extraction
7. Commit each extraction separately

**Verification:** main.rs should be under 400 lines when done. Each extracted module under 300 lines.

## Phase 2: Fix Known Bugs

### Bug 1: Render flickering
The instance buffer write was moved before the render pass, but flickering persists. Investigate:
- Read `crates/sa_render/src/renderer.rs` completely
- Check if the instance buffer is being double-buffered properly
- Check if the `GpuProfiler` begin_query/end_query calls inside the render pass cause issues on Metal
- Try disabling ALL profiler queries (comment them out) and see if the issue is in the profiler
- If profiler is the cause, gate profiler queries behind a runtime flag
- Write a test if possible

### Bug 2: Terrain visibility during cruise approach
The planet disappears at ~60km during cruise approach. The `flush_for_teleport()` was added on cruise disengage but may not be working:
- Read the cruise disengage code path
- Verify `flush_for_teleport()` is actually called (add a log)
- Check if the icosphere is being hidden too early
- The `visible_in_gpu` count check may be wrong — stale chunks in `gpu_meshes` inflate the count
- Consider: on cruise disengage, also clear the streaming cache and force regeneration at the new position

### Bug 3: Surface collision in-game
The headless test proves collision works. But in-game the ship may still fall through. Differences between test and game:
- Test uses simple Y-axis gravity. Game uses 3D gravity toward planet center.
- Test has no terrain body rotation issues. Game has anchor rebasing.
- Add diagnostic: log the ship-to-barrier distance every frame when altitude < 10km
- If the barrier works but the ship stops 50m above ground (due to barrier center offset), adjust the barrier position

## Phase 3: Renderer Optimization

### 3A: Shared terrain index buffer
All 33x33 terrain chunks use the same grid topology. Currently each chunk uploads its own index buffer. Create ONE shared index buffer:
- Generate the 33x33 grid indices once during initialization
- Store as a shared buffer in MeshStore or Renderer
- When uploading terrain chunks, only upload vertex data (skip indices)
- Bind the shared index buffer for all terrain draw calls

### 3B: Verify draw batching works correctly
- Add a frame counter log: "BATCH_DIAG: N draw commands, M batches, K unique meshes"
- Terrain chunks with the same LOD share the same mesh topology — verify they batch correctly
- If all terrain chunks have unique mesh handles (because each is uploaded separately), batching won't help. In that case, consider uploading vertices into a shared vertex buffer with per-chunk offsets.

## Phase 4: Terrain System Improvements

### 4A: GPU-side LOD morphing (from Terra study)
Currently terrain LOD transitions show visible seams hidden by skirts. Add vertex shader morphing:
- In the terrain vertex shader (or geometry.wgsl since terrain uses the same pipeline), add a morph uniform
- Each terrain chunk passes its morph factor (0.0 = full detail, 1.0 = parent LOD)
- Odd-indexed vertices morph toward the midpoint of their even neighbors
- `morphed_pos = mix(fine_pos, coarse_pos, morph_factor)`
- This requires passing morph_factor as an instance attribute or uniform

### 4B: Frustum culling
The quadtree doesn't do frustum culling — all in-range nodes are emitted. Add basic frustum culling:
- Extract frustum planes from the view-projection matrix
- In `select_visible_nodes`, reject nodes whose bounding sphere is outside the frustum
- This reduces draw commands by ~50% (back-hemisphere chunks are currently rendered)

## Phase 5: Sky and Visual Improvements

### 5A: Review and improve sky shaders
- Read all shader files in `crates/sa_render/src/shaders/`
- Check for any visual artifacts or improvements possible
- The sky shader renders a procedural galaxy — verify it looks correct
- Star field rendering — check billboard sizing and brightness
- Nebula rendering — check alpha blending

### 5B: Atmosphere rendering improvements
- The atmosphere is currently a solid-color icosphere shell
- Consider: simple Rayleigh scattering approximation in the fragment shader
- Even a basic `exp(-altitude * density)` fog gives depth

## Phase 6: Testing and Validation

- Run ALL tests: `cargo test --workspace`
- Run clippy: `cargo clippy --workspace -- -D warnings`
- Build release: `cargo build -p spaceaway --release`
- Verify the game compiles and runs: `cargo build -p spaceaway`
- Write final report

## Phase 7: Continuous Improvement Loop (RUNS UNTIL SESSION ENDS)

After all phases complete, enter an infinite improvement loop. Each iteration:

### Step A: Audit
Pick ONE system from this rotating list (cycle through them):
1. Terrain pipeline (streaming, LOD, chunk generation, draw commands)
2. Physics pipeline (collision, barrier, heightfield, rebase, gravity)
3. Renderer (shaders, draw calls, buffers, depth, camera)
4. Game systems (drive, landing, interaction, navigation, audio)
5. Code quality (dead code, TODOs, unclear logic, missing error handling)

### Step B: Investigate
- Read EVERY file related to that system
- Trace data flow end-to-end
- Run `cargo test` to check current state
- Look for: logic errors, precision issues, performance problems, missing edge cases, code that contradicts CLAUDE.md conventions, files over 300 lines

### Step C: Validate
For each issue found:
- **Prove it** — show the exact code, the exact values, the exact failure
- **Research the fix** — read documentation, check how other engines handle it
- **Write a test first** that fails before the fix and passes after
- Do NOT fix anything you cannot prove is wrong

### Step D: Fix
- Apply the fix
- Run `cargo clippy --workspace -- -D warnings`
- Run `cargo test --workspace`
- Commit with clear message explaining what was wrong and why the fix is correct
- Append findings to the report file

### Step E: Repeat
Go back to Step A with the next system in the rotation. Continue until the session ends.

### What counts as an issue worth fixing:
- Code that produces incorrect results (proven with values)
- Performance bottlenecks (proven with profiling data or algorithmic analysis)
- Files over 300 lines (project convention violation)
- Dead code, unused imports, stale comments
- Missing test coverage for critical paths
- Unsafe patterns that have safe alternatives
- Hardcoded values that should be constants or config

### What does NOT count:
- "I think this might be wrong" — prove it or skip it
- Style preferences without functional impact
- Refactoring that doesn't improve readability or correctness
- Adding features not in the spec

## Time Estimates

The phase times are ESTIMATES, not limits. If you finish early, move to the next phase. Phase 7 fills ALL remaining time. There is no idle state — always be working on the next improvement iteration.

## Progress Report Format

After each phase, append to `/Users/dante/Projects/SpaceAway/docs/overnight-report.md`:

```markdown
## Phase N: [Name] — [COMPLETE/PARTIAL/SKIPPED]
**Time:** [timestamp]
**Changes:**
- [commit hash] [description]
- [commit hash] [description]

**Findings:**
- [what was discovered]

**Issues:**
- [anything unresolved]

**Tests:** [pass count] / [total]
```

## How to Work

1. Start with Phase 1 (main.rs refactor) — it's the biggest win for code quality
2. After each phase, run tests and clippy, commit, update report
3. If a phase is blocked (e.g., needs GPU testing you can't do), document it and move to the next
4. If you find bugs during refactoring, fix them immediately and add tests
5. Use subagents (`Agent` tool) for independent research tasks
6. Use `cargo check` frequently for fast feedback
7. Prefer small, focused commits over large ones
8. When in doubt, read the code before changing it

## DO NOT

- Do not add new features not listed above
- Do not refactor crates other than `spaceaway` in Phase 1 (they're already well-structured, Phase 7 may touch them if issues are proven)
- Do not modify test files unless tests fail
- Do not skip the clippy/test checks after changes
- Do not make assumptions about what code does — read it first

## Start Now

Begin with Phase 1. Read main.rs completely, plan the extraction, then execute it methodically. Write the first progress report entry when Phase 1 is complete.
