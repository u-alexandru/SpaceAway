# Audio System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a spatial audio engine (`sa_audio`) with 5 channels (ambience, engine, music, SFX, voice), 3D positioning, computer voice copilot, context-driven music, and atmospheric ship sounds.

**Architecture:** New `sa_audio` crate using `rodio` for WAV playback. AudioManager owns an `OutputStream` and manages channel state. Game binary calls high-level API each frame. Sound files curated from asset library into `resources/sounds/` (committed).

**Tech Stack:** `rodio` (audio playback), `glam` (3D math), `rand` (random intervals/track selection), WAV files.

**Spec:** `docs/superpowers/specs/2026-03-28-audio-system-design.md`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `Cargo.toml` (workspace) | Modify | Add sa_audio member + rodio/rand deps |
| `crates/sa_audio/Cargo.toml` | Create | Crate manifest |
| `crates/sa_audio/src/lib.rs` | Create | AudioManager public API |
| `crates/sa_audio/src/channel.rs` | Create | Channel types, crossfade logic |
| `crates/sa_audio/src/spatial.rs` | Create | Listener, 3D attenuation, stereo pan |
| `crates/sa_audio/src/catalog.rs` | Create | SoundId enums, file path mapping |
| `resources/sounds/` | Create | Curated WAV files from library |
| `crates/spaceaway/Cargo.toml` | Modify | Add sa_audio dependency |
| `crates/spaceaway/src/main.rs` | Modify | Wire AudioManager into game loop |

---

### Task 1: Create sa_audio crate skeleton + curate sound files

**Files:**
- Create: `crates/sa_audio/Cargo.toml`
- Create: `crates/sa_audio/src/lib.rs`
- Create: `crates/sa_audio/src/catalog.rs`
- Modify: `Cargo.toml` (workspace)
- Create: `resources/sounds/` (curated WAV files)

- [ ] **Step 1: Create crate directory and Cargo.toml**

```toml
# crates/sa_audio/Cargo.toml
[package]
name = "sa_audio"
version.workspace = true
edition.workspace = true

[dependencies]
rodio = { version = "0.19", default-features = false, features = ["wav"] }
glam = { workspace = true }
rand = "0.8"
log = { workspace = true }
```

- [ ] **Step 2: Add to workspace Cargo.toml**

Add `"crates/sa_audio"` to the `[workspace] members` list.
Add to `[workspace.dependencies]`:
```toml
rodio = { version = "0.19", default-features = false, features = ["wav"] }
rand = "0.8"
sa_audio = { path = "crates/sa_audio" }
```

- [ ] **Step 3: Create catalog.rs with sound ID enums**

```rust
//! Sound catalog: maps logical sound IDs to file paths.

/// Sound effects (one-shot, spatial).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SfxId {
    ButtonClick,
    ButtonToggle,
    LeverMove,
    Confirm,
    DoorOpen,
    DoorClose,
}

/// Computer voice announcements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VoiceId {
    EngagingWarp,
    AllSystemsReady,
    EnergyLow,
    Danger,
    Error,
    EnginesIgniting,
    Alert,
    SystemsOnline,
}

/// Voice priority (higher interrupts lower).
impl VoiceId {
    pub fn priority(self) -> u8 {
        match self {
            Self::Danger | Self::Error => 3,       // Critical
            Self::EnergyLow | Self::Alert => 2,    // High
            Self::EngagingWarp | Self::AllSystemsReady | Self::SystemsOnline => 1, // Medium
            Self::EnginesIgniting => 0,             // Low
        }
    }
}

/// Alarm sounds (looping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlarmId {
    FuelLow,
    FuelCritical,
    PowerFailure,
}

/// Music contexts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MusicContext {
    Idle,
    Exploration,
    Warp,
    Tension,
    Discovery,
}

/// Engine sound states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineState {
    Off,
    Idle,
    Impulse,
    Cruise,
    WarpSpool,
    WarpEngaged,
}

/// Maps sound IDs to file paths (relative to sounds root).
pub fn sfx_path(id: SfxId) -> &'static str {
    match id {
        SfxId::ButtonClick => "interface/button_click.wav",
        SfxId::ButtonToggle => "interface/button_toggle.wav",
        SfxId::LeverMove => "interface/lever_move.wav",
        SfxId::Confirm => "interface/confirm.wav",
        SfxId::DoorOpen => "doors/door_open.wav",
        SfxId::DoorClose => "doors/door_close.wav",
    }
}

pub fn voice_path(id: VoiceId) -> &'static str {
    match id {
        VoiceId::EngagingWarp => "voice/engaging_warp.wav",
        VoiceId::AllSystemsReady => "voice/all_systems_ready.wav",
        VoiceId::EnergyLow => "voice/energy_low.wav",
        VoiceId::Danger => "voice/danger.wav",
        VoiceId::Error => "voice/error.wav",
        VoiceId::EnginesIgniting => "voice/engines_igniting.wav",
        VoiceId::Alert => "voice/alert.wav",
        VoiceId::SystemsOnline => "voice/systems_online.wav",
    }
}

pub fn alarm_path(id: AlarmId) -> &'static str {
    match id {
        AlarmId::FuelLow => "alarms/fuel_low.wav",
        AlarmId::FuelCritical => "alarms/fuel_critical.wav",
        AlarmId::PowerFailure => "alarms/power_failure.wav",
    }
}

pub fn engine_path(state: EngineState) -> Option<&'static str> {
    match state {
        EngineState::Off => None,
        EngineState::Idle => Some("engine/idle_inside.wav"),
        EngineState::Impulse => Some("engine/impulse_inside.wav"),
        EngineState::Cruise => Some("engine/cruise_loop.wav"),
        EngineState::WarpSpool => Some("warp/spool.wav"),
        EngineState::WarpEngaged => Some("engine/warp_loop.wav"),
    }
}

/// Music track lists per context.
pub fn music_tracks(ctx: MusicContext) -> &'static [&'static str] {
    match ctx {
        MusicContext::Idle => &[
            "music/Alone.wav", "music/Winter.wav", "music/Tears.wav", "music/winterdreams.wav",
        ],
        MusicContext::Exploration => &[
            "music/SilentFloating.wav", "music/Deep.wav", "music/Hope.wav",
            "music/Freedom.wav", "music/Forever.wav",
        ],
        MusicContext::Warp => &[
            "music/fly.wav", "music/Spherical.wav", "music/sound.wav", "music/Freak.wav",
        ],
        MusicContext::Tension => &[
            "music/trapped.wav", "music/BehindYou.wav", "music/dark.wav", "music/mindcontrol.wav",
        ],
        MusicContext::Discovery => &[
            "music/Fantasy.wav", "music/reflexions.wav", "music/sence.wav",
        ],
    }
}

pub fn ambience_hum_path() -> &'static str { "ambience/ship_hum.wav" }
pub fn ambience_life_support_path() -> &'static str { "ambience/life_support.wav" }
pub fn ambience_void_drone_path() -> &'static str { "ambience/void_drone.wav" }
pub fn ambience_creak_paths() -> &'static [&'static str] {
    &["ambience/creak_01.wav", "ambience/creak_02.wav", "ambience/creak_03.wav",
      "ambience/creak_04.wav", "ambience/creak_05.wav"]
}
pub fn warp_disengage_path() -> &'static str { "warp/disengage.wav" }
```

- [ ] **Step 4: Create minimal lib.rs**

```rust
//! sa_audio: spatial audio engine for SpaceAway.

pub mod catalog;
pub mod channel;
pub mod spatial;

pub use catalog::{SfxId, VoiceId, AlarmId, MusicContext, EngineState};
```

- [ ] **Step 5: Create stub channel.rs and spatial.rs**

```rust
// crates/sa_audio/src/channel.rs
//! Audio channel management and crossfade logic.
```

```rust
// crates/sa_audio/src/spatial.rs
//! 3D audio positioning: listener, emitters, distance attenuation.

use glam::Vec3;

/// Listener state (camera position + orientation).
pub struct Listener {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
        }
    }
}

/// Compute volume and pan for a sound at `source_pos` relative to the listener.
/// Returns (volume_multiplier, pan) where pan is -1.0 (left) to 1.0 (right).
pub fn spatial_params(listener: &Listener, source_pos: Vec3, max_range: f32) -> (f32, f32) {
    let to_source = source_pos - listener.position;
    let distance = to_source.length();

    if distance < 0.001 {
        return (1.0, 0.0); // at listener position
    }

    // Distance attenuation: linear falloff
    let volume = (1.0 - distance / max_range).clamp(0.0, 1.0);

    // Stereo pan: project onto listener's right vector
    let right = listener.forward.cross(listener.up).normalize();
    let dir = to_source / distance;
    let pan = dir.dot(right).clamp(-1.0, 1.0);

    (volume, pan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_listener_full_volume_center_pan() {
        let listener = Listener::default();
        let (vol, pan) = spatial_params(&listener, Vec3::ZERO, 30.0);
        assert!((vol - 1.0).abs() < 0.01);
        assert!(pan.abs() < 0.01);
    }

    #[test]
    fn beyond_range_silent() {
        let listener = Listener::default();
        let (vol, _) = spatial_params(&listener, Vec3::new(0.0, 0.0, -50.0), 30.0);
        assert!(vol < 0.01);
    }

    #[test]
    fn right_side_positive_pan() {
        let listener = Listener::default();
        let (_, pan) = spatial_params(&listener, Vec3::new(10.0, 0.0, 0.0), 30.0);
        assert!(pan > 0.3, "right-side sound should have positive pan, got {pan}");
    }

    #[test]
    fn left_side_negative_pan() {
        let listener = Listener::default();
        let (_, pan) = spatial_params(&listener, Vec3::new(-10.0, 0.0, 0.0), 30.0);
        assert!(pan < -0.3, "left-side sound should have negative pan, got {pan}");
    }
}
```

- [ ] **Step 6: Curate sound files**

Copy selected WAV files from `assets/sound/SpaceSFXandMusic/` to `resources/sounds/`:

```bash
mkdir -p resources/sounds/{engine,ambience,interface,doors,voice,music,alarms,warp}

# Engine loops (inside variants for cockpit perspective)
cp "assets/sound/SpaceSFXandMusic/Engine Sounds/engine01_loop_inside.wav" resources/sounds/engine/idle_inside.wav
cp "assets/sound/SpaceSFXandMusic/Engine Sounds/engine03_loop_inside.wav" resources/sounds/engine/impulse_inside.wav
cp "assets/sound/SpaceSFXandMusic/Engine Sounds/engine05_loop_inside.wav" resources/sounds/engine/cruise_loop.wav
cp "assets/sound/SpaceSFXandMusic/Engine Sounds/engine10_loop_inside.wav" resources/sounds/engine/warp_loop.wav

# Ambience
cp "assets/sound/SpaceSFXandMusic/Atmos/BackgroundSound06_loop.wav" resources/sounds/ambience/ship_hum.wav
cp "assets/sound/SpaceSFXandMusic/Atmos/BackgroundSound08_loop.wav" resources/sounds/ambience/life_support.wav
cp "assets/sound/SpaceSFXandMusic/Atmos/BackgroundSound01.wav" resources/sounds/ambience/void_drone.wav
# Ship creaks
for i in 1 2 3 4 5; do cp "assets/sound/SpaceSFXandMusic/Ship Sounds/Steel0${i}.wav" "resources/sounds/ambience/creak_0${i}.wav" 2>/dev/null; done

# Interface
cp "assets/sound/SpaceSFXandMusic/Interface/bing01.wav" resources/sounds/interface/button_click.wav
cp "assets/sound/SpaceSFXandMusic/Interface/bing03.wav" resources/sounds/interface/button_toggle.wav
cp "assets/sound/SpaceSFXandMusic/Interface/bing05.wav" resources/sounds/interface/lever_move.wav
cp "assets/sound/SpaceSFXandMusic/Interface/bing07.wav" resources/sounds/interface/confirm.wav

# Doors
cp "assets/sound/SpaceSFXandMusic/Doors/automaticdoor01_open.wav" resources/sounds/doors/door_open.wav
cp "assets/sound/SpaceSFXandMusic/Doors/automaticdoor01_close.wav" resources/sounds/doors/door_close.wav

# Computer voice (voice01)
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/engagingwarpdrive.wav" resources/sounds/voice/engaging_warp.wav
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/allsystemsarefunctioningperfectly.wav" resources/sounds/voice/all_systems_ready.wav
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/energyrunninglowsoon.wav" resources/sounds/voice/energy_low.wav
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/danger.wav" resources/sounds/voice/danger.wav
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/error.wav" resources/sounds/voice/error.wav
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/enginesignitingstandbyforlaunch.wav" resources/sounds/voice/engines_igniting.wav
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/alert.wav" resources/sounds/voice/alert.wav
cp "assets/sound/SpaceSFXandMusic/Computervoices/voice01/allsystemsreadyforliftoff.wav" resources/sounds/voice/systems_online.wav

# Warp effects
cp "assets/sound/SpaceSFXandMusic/Movements/Transform01.wav" resources/sounds/warp/spool.wav
cp "assets/sound/SpaceSFXandMusic/Movements/Transform02.wav" resources/sounds/warp/disengage.wav

# Alarms
cp "assets/sound/SpaceSFXandMusic/Alarms/alarm01.wav" resources/sounds/alarms/fuel_low.wav
cp "assets/sound/SpaceSFXandMusic/Alarms/alarm06.wav" resources/sounds/alarms/fuel_critical.wav
cp "assets/sound/SpaceSFXandMusic/Alarms/alarm10 loop.wav" resources/sounds/alarms/power_failure.wav

# Music (all 30 tracks)
cp assets/sound/SpaceSFXandMusic/Music/*.wav resources/sounds/music/
```

Note: exact source files may need adjustment after auditioning. The _inside variants are preferred for engine sounds (muffled cockpit perspective).

- [ ] **Step 7: Build and verify**

Run: `cargo build -p sa_audio`
Expected: compiles with no errors

- [ ] **Step 8: Run tests**

Run: `cargo test -p sa_audio`
Expected: spatial audio tests pass

- [ ] **Step 9: Commit**

```bash
git add crates/sa_audio/ Cargo.toml resources/sounds/
git commit -m "feat(audio): sa_audio crate skeleton + curated sound library"
```

---

### Task 2: AudioManager with rodio playback

**Files:**
- Modify: `crates/sa_audio/src/lib.rs`
- Modify: `crates/sa_audio/src/channel.rs`

- [ ] **Step 1: Implement AudioManager core**

`lib.rs`:
```rust
//! sa_audio: spatial audio engine for SpaceAway.

pub mod catalog;
pub mod channel;
pub mod spatial;

pub use catalog::{SfxId, VoiceId, AlarmId, MusicContext, EngineState};
use channel::Channels;
use spatial::Listener;
use glam::Vec3;
use std::path::PathBuf;

pub struct AudioManager {
    channels: Channels,
    listener: Listener,
    sounds_root: PathBuf,
    master_volume: f32,
}

impl AudioManager {
    pub fn new(sounds_path: &str) -> Self {
        Self {
            channels: Channels::new(),
            listener: Listener::default(),
            sounds_root: PathBuf::from(sounds_path),
            master_volume: 1.0,
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.channels.update(dt, &self.sounds_root, &self.listener, self.master_volume);
    }

    pub fn set_listener(&mut self, pos: Vec3, forward: Vec3, up: Vec3) {
        self.listener.position = pos;
        self.listener.forward = forward;
        self.listener.up = up;
    }

    pub fn set_engine_state(&mut self, engine_state: EngineState) {
        self.channels.set_engine_state(engine_state, &self.sounds_root);
    }

    pub fn set_music_context(&mut self, ctx: MusicContext) {
        self.channels.set_music_context(ctx);
    }

    pub fn set_power(&mut self, power_on: bool) {
        self.channels.set_power(power_on, &self.sounds_root);
    }

    pub fn play_sfx(&mut self, id: SfxId, position: Vec3) {
        self.channels.play_sfx(id, Some(position), &self.sounds_root, &self.listener);
    }

    pub fn play_sfx_global(&mut self, id: SfxId) {
        self.channels.play_sfx(id, None, &self.sounds_root, &self.listener);
    }

    pub fn announce(&mut self, id: VoiceId) {
        self.channels.announce(id, &self.sounds_root);
    }

    pub fn play_alarm(&mut self, id: AlarmId) {
        self.channels.play_alarm(id, &self.sounds_root);
    }

    pub fn clear_alarm(&mut self, id: AlarmId) {
        self.channels.clear_alarm(id);
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }
}
```

- [ ] **Step 2: Implement Channels**

`channel.rs` — manages the rodio OutputStream and all channel sinks. This is the largest file. Key components:

- `rodio::OutputStream` + `OutputStreamHandle` (kept alive for the lifetime of Channels)
- Engine channel: one `Sink` for the current loop. On state change, stop old sink, start new one with crossfade (fade out old over 1s, start new at 0 volume, fade in over 1s).
- Ambience channel: 2-3 sinks for hum + life support + void drone. Random creak timer.
- Music channel: one sink for current track. Timer for silence gaps. Random track selection.
- SFX: create a new `Sink` per one-shot sound. Apply volume/pan from spatial params. Let rodio clean up when done.
- Voice: one sink. Queue of pending VoiceId. When current finishes, play next in queue (highest priority first).
- Alarm: HashMap of AlarmId → Sink (looping).

The implementer should use `rodio::Decoder::new(BufReader::new(File::open(path)))` for loading WAV files. Loop via `sink.append(source.repeat_infinite())` for loops.

Crossfade: track two volumes (old fading out, new fading in) and update each frame in `update(dt)`.

- [ ] **Step 3: Build and test**

Run: `cargo build -p sa_audio`
Run: `cargo test -p sa_audio`

- [ ] **Step 4: Commit**

```bash
git add crates/sa_audio/
git commit -m "feat(audio): AudioManager with rodio playback, all channels"
```

---

### Task 3: Wire audio into game loop

**Files:**
- Modify: `crates/spaceaway/Cargo.toml`
- Modify: `crates/spaceaway/src/main.rs`

- [ ] **Step 1: Add sa_audio dependency**

In `crates/spaceaway/Cargo.toml`, add:
```toml
sa_audio = { workspace = true }
```

- [ ] **Step 2: Add AudioManager to App struct**

Add field: `audio: sa_audio::AudioManager`
Initialize: `audio: sa_audio::AudioManager::new("resources/sounds")`

- [ ] **Step 3: Update listener each frame**

In the render section (after camera is computed), add:
```rust
let fwd = self.camera.forward();
let right = self.camera.right();
let up = fwd.cross(right).normalize(); // or Vec3::Y for simple
self.audio.set_listener(
    Vec3::new(cam_pos.x as f32, cam_pos.y as f32, cam_pos.z as f32),
    fwd,
    up,
);
```

- [ ] **Step 4: Wire engine state**

After drive state updates, compute engine state:
```rust
let engine_state = if !ship.engine_on {
    sa_audio::EngineState::Off
} else if self.drive.mode() == DriveMode::Warp {
    if matches!(self.drive.status(), DriveStatus::Spooling(_)) {
        sa_audio::EngineState::WarpSpool
    } else {
        sa_audio::EngineState::WarpEngaged
    }
} else if self.drive.mode() == DriveMode::Cruise {
    sa_audio::EngineState::Cruise
} else if ship.throttle > 0.01 {
    sa_audio::EngineState::Impulse
} else {
    sa_audio::EngineState::Idle
};
self.audio.set_engine_state(engine_state);
```

- [ ] **Step 5: Wire music context**

```rust
let music_ctx = if self.drive.mode() == DriveMode::Warp
    && matches!(self.drive.status(), DriveStatus::Engaged) {
    sa_audio::MusicContext::Warp
} else if self.ship_resources.fuel < 0.2 || self.ship_resources.exotic_fuel < 0.1 {
    sa_audio::MusicContext::Tension
} else if self.active_system.is_some() {
    sa_audio::MusicContext::Exploration
} else {
    sa_audio::MusicContext::Idle
};
self.audio.set_music_context(music_ctx);
```

- [ ] **Step 6: Wire voice announcements**

At each trigger point in main.rs:
- Warp spool start (key 3 handler): `self.audio.announce(VoiceId::EngagingWarp);`
- System enter (gravity well): `self.audio.announce(VoiceId::AllSystemsReady);`
- Fuel low (resource update): `self.audio.announce(VoiceId::EnergyLow);` (with cooldown to avoid spam)
- Engine start (interaction): `self.audio.announce(VoiceId::EnginesIgniting);`

- [ ] **Step 7: Wire SFX**

At interaction events:
- Button click: `self.audio.play_sfx(SfxId::ButtonClick, button_world_pos);`
- Lever move: `self.audio.play_sfx(SfxId::LeverMove, lever_world_pos);`

- [ ] **Step 8: Wire power state**

```rust
self.audio.set_power(self.ship_resources.power > 0.0);
```

- [ ] **Step 9: Call update each frame**

```rust
self.audio.update(dt);
```

- [ ] **Step 10: Build and test manually**

Run: `cargo run -p spaceaway`
Expected: hear ship ambience on start, engine sounds when throttle changes, voice when engaging warp, music playing softly.

- [ ] **Step 11: Commit**

```bash
git add crates/spaceaway/
git commit -m "feat: wire audio into game loop — engine, voice, music, SFX"
```

---

### Task 4: Polish and documentation

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`

- [ ] **Step 3: Update CLAUDE.md**

Add to the Architecture section:
```
Engine:       sa_audio          — spatial audio, channels, voice, music
```

Add to Key Bindings or relevant section any audio-related info.

- [ ] **Step 4: Update performance-techniques.md if needed**

Document audio performance characteristics (rodio runs on separate thread, no frame budget impact).

- [ ] **Step 5: Commit and push**

```bash
git add -A
git commit -m "docs: audio system documentation"
git push
```

---

## Summary

| Task | What it builds | Key files |
|------|---------------|-----------|
| 1 | Crate skeleton + sound catalog + curated WAV files | `sa_audio/`, `resources/sounds/` |
| 2 | AudioManager with all 5 channels via rodio | `lib.rs`, `channel.rs` |
| 3 | Game loop integration — engine, voice, music, SFX wiring | `main.rs` |
| 4 | Testing + documentation | CLAUDE.md, tests |

**Tasks 1-2 are library code** (pure audio, no game dependency). Fully testable in isolation.
**Task 3 is integration** (wires into main.rs). Requires judgment about trigger placement.
**Task 4 is polish.**
