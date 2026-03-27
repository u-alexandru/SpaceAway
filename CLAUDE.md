# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

SpaceAway is a cooperative first-person space exploration game with a custom Rust engine. 1-4 players crew a ship exploring a procedurally generated infinite universe. The engine is designed for AI-agent-driven development.

See `docs/superpowers/specs/2026-03-27-spaceaway-engine-design.md` for the full design spec.

## Build Commands

```bash
cargo check                    # Fast type checking (use first, always)
cargo build -p spaceaway       # Build the game binary
cargo run -p spaceaway         # Run the game
cargo test --workspace         # Run all tests
cargo test -p sa_physics       # Run tests for a single crate
cargo clippy --workspace -- -D warnings  # Lint (run before committing)
```

## Architecture

Cargo workspace with layered crates. Dependencies flow downward only.

```
Application:  spaceaway (game binary)
Game Logic:   sa_ship, sa_survival, sa_universe, sa_player
Engine:       sa_render, sa_physics, sa_net, sa_audio, sa_input
Core:         sa_ecs, sa_math, sa_core
```

All crates live in `crates/`. The `sa_` prefix is mandatory for all crate names.

### Cross-Crate Communication

Crates never call each other directly. They communicate through `sa_core::EventBus` — emit strongly-typed events, consume them in other systems. This keeps dependencies clean and one-directional.

### Coordinate System

- **Simulation:** `WorldPos` (f64) — double-precision, used everywhere in game logic
- **Rendering:** `LocalPos` (f32) — camera-relative, converted from WorldPos via origin rebasing
- Never use bare f32/f64 for positions. Always use the typed wrappers.

### Unit Types

Use strong types from `sa_math::units`: `Meters`, `Seconds`, `Watts`, `Kilograms`, `Newtons`, `Kelvin`, `Liters`, `MetersPerSecond`. Never use bare numbers for physical quantities.

### ECS

Based on hecs. Components are plain structs. Systems are functions that take `(&mut GameWorld, &mut EventBus, &FrameTime)`. Register systems in the `Schedule`.

## Conventions

- Every file stays under 300 lines. Split when it grows.
- One concept per file.
- Public API at the top of each file, private internals below.
- All config in RON/TOML. No binary formats in the repo.
- Shaders in WGSL.
- `thiserror` for error types per crate. No string errors, no panics in library code.
- Unit tests inline with `#[cfg(test)]`. Integration tests in `tests/`.
- Run `cargo clippy` before every commit.

## Platform

Primary target: macOS (Metal via wgpu). Secondary: Windows (DX12/Vulkan via wgpu). The same code runs on both — wgpu abstracts the graphics backend.
