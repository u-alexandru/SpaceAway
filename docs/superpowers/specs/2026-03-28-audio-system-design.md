# Audio System Design (`sa_audio`)

Spatial audio engine with layered channels, computer voice announcements, context-driven music, and the silence of deep space.

---

## 1. Architecture

New `sa_audio` crate at the Engine layer. Depends on `rodio` for WAV playback. The game binary sends high-level commands. Audio crate handles mixing, spatial math, crossfading, and voice queuing internally.

```
sa_audio/
  src/
    lib.rs          — AudioManager, public API
    channel.rs      — Channel types (ambience, engine, music, sfx, voice)
    spatial.rs      — 3D position, listener, distance attenuation, stereo panning
    catalog.rs      — SoundId enums, maps IDs to file paths
```

Crate dependencies: `rodio`, `glam` (for Vec3 positions), `rand` (for random creaks/silence gaps).

---

## 2. Sound Channels

Five simultaneous layers, mixed together:

| Channel | Type | Spatial? | Polyphony | Description |
|---------|------|----------|-----------|-------------|
| **Ambience** | Looping | No (ship-wide) | 1-3 layers | Ship hum + void undertone + random creaks |
| **Engine** | Looping, crossfade | No (ship-wide) | 1 (crossfade between states) | Engine sound changes with drive mode/throttle |
| **Music** | One-shot/looping | No | 1 (crossfade between tracks) | Context-driven playlist with silence gaps |
| **SFX** | One-shot | **Yes (3D)** | 8+ simultaneous | Button clicks, doors, impacts. Position-based. |
| **Voice** | One-shot, queued | No (ship-wide) | 1 (priority queue) | Computer announcements, one at a time |

### Channel Volumes (default, user-adjustable later)

| Channel | Volume | Notes |
|---------|--------|-------|
| Ambience | 0.3 | Subtle, always present |
| Engine | 0.5 | Scales with throttle |
| Music | 0.25 | Background, never dominant |
| SFX | 0.7 | Clear, immediate feedback |
| Voice | 0.8 | Must be heard over everything |

---

## 3. Spatial Audio

### Listener
- Position: camera world position (updated each frame)
- Orientation: camera forward + up vectors
- Updated via `audio.set_listener(pos, forward, up)`

### Emitters
- SFX sounds have an optional world position
- If no position: plays at full volume (ship-wide, like ambience)
- If positioned: volume and pan computed from listener

### Distance Attenuation
- Model: `volume = clamp(1.0 - distance / max_range, 0.0, 1.0)`
- Ship interior max range: 30m (ship is ~38m long)
- Minimum volume: 0.0 (fully silent beyond range)
- Reference distance: 1m (full volume within 1m)

### Stereo Panning
- Compute angle between listener forward and direction to sound
- Pan: `sin(angle)` — left/right based on angle
- Simple stereo, no HRTF

### Future: Occlusion
- Defer wall-based occlusion to later phase
- Would muffle sounds through bulkheads

---

## 4. Engine Sound States

| State | Loop Sound | Behavior |
|-------|-----------|----------|
| Engine off | Silence | — |
| Engine on, idle | `engine/idle_inside.wav` | Low hum, quiet |
| Impulse thrust | `engine/impulse_inside.wav` | Volume scales with throttle 0→1 |
| Cruise engaged | `engine/cruise_loop.wav` | Crossfade 1s from impulse. Higher pitch. |
| Warp spooling | `warp/spool.wav` one-shot over current | Building charge sound (5 seconds) |
| Warp engaged | `engine/warp_loop.wav` | Crossfade 0.5s. Deep bass rumble. |
| Warp disengage | `warp/disengage.wav` one-shot | Deceleration whoosh, fade to idle |

Crossfade duration: 1 second between engine states. Warp engage flash: 0.5s.

---

## 5. Computer Voice (voice01, moderate callouts)

Single voice variant (voice01). ~15 trigger events:

| Event | Voice Line | Priority |
|-------|-----------|----------|
| Warp spool start | "engaging warp drive" | Medium |
| Warp engaged | "warp drive activated" or similar | Medium |
| Warp disengage (manual) | — (just SFX) | — |
| Warp emergency drop | "danger" | High |
| Enter star system | "all systems ready" | Medium |
| Fuel low (20%) | "energy running low" | High |
| Fuel critical (5%) | "danger" | Critical |
| Exotic fuel empty | "error" | Critical |
| Engine start | "engines igniting standby for launch" | Low |
| Target locked | "target locked" (if we have it) | Low |
| Power failure | "emergency evacuation protocols activated" | Critical |
| System boot | "all systems are functioning perfectly" | Low |

Priority levels: Critical > High > Medium > Low. Higher priority interrupts current voice. Same priority queues behind current.

### Future voice triggers (documented, not implemented now)
- "docking maneuver initiated" — when docking
- "entering hostile territory" — combat zones
- "hull integrity compromised" — damage
- "approaching landing zone" — planet landing

---

## 6. Music System

### Context-driven playlists

| Context | Trigger | Tracks |
|---------|---------|--------|
| **Idle** | Menu, ship stationary >60s | Alone, Winter, Tears, winterdreams |
| **Exploration** | In system, cruising | SilentFloating, Deep, Hope, Freedom, Forever |
| **Warp** | Warp drive engaged | fly, Spherical, sound, Freak |
| **Tension** | Low fuel (<20%), stranded | trapped, BehindYou, dark, mindcontrol |
| **Discovery** | Entering new system | Fantasy, reflexions, sence |

### Behavior
- On context change: crossfade current track over 3 seconds
- Between tracks: 30-90 seconds of silence (random)
- Track selection: random within context pool, no immediate repeats
- Volume: 0.25 base, ducked to 0.15 during voice announcements

---

## 7. Silence of Space

### Ship alive (power on)
- **Base hum**: continuous low-frequency loop (`ambience/ship_hum.wav`). Volume 0.2-0.3.
- **Life support**: subtle fan/air circulation loop layered on hum. Volume 0.1.
- **Random creaks**: ship structure settling sounds (`ambience/creak_01-05.wav`). Random interval 30-120 seconds. Very low volume (0.05-0.1). Adds life without being noticeable.

### Void undertone
- Barely perceptible low drone. Volume 0.03-0.05.
- Increases slightly (to 0.08) when camera faces outward through windows (dot product of camera forward and ship outward normal).
- Decreases when looking at ship interior.
- Frequency: very low, ~40-60 Hz feel.

### Power failure
- Ship hum fades to silence over 3 seconds
- Life support stops
- Void undertone persists alone for 5 seconds, then also fades
- True silence for 5-10 seconds
- Single structural creak breaks the silence
- Terrifying.

---

## 8. Sound File Organization

Curated sounds copied from `assets/sound/` (gitignored library) to `resources/sounds/` (committed, ~50-80 files):

```
resources/sounds/
  engine/
    idle_inside.wav          — from Engine Sounds/engine01_loop_inside.wav
    impulse_inside.wav       — from Engine Sounds/engine02_loop_inside.wav
    cruise_loop.wav          — from Engine Sounds/Engine03Loop.wav (or similar)
    warp_loop.wav            — from Atmos/BackgroundSound06_loop.wav (deep rumble)
  ambience/
    ship_hum.wav             — from Atmos/BackgroundSound01.wav
    life_support.wav         — from Atmos/BackgroundSound08_loop.wav
    void_drone.wav           — from Atmos/ (select deepest, lowest tone)
    creak_01.wav - creak_05.wav — from Ship Sounds/Steel01-05.wav
  interface/
    button_click.wav         — from Interface/bing01.wav
    button_toggle.wav        — from Interface/bing03.wav
    lever_move.wav           — from Interface/ (select mechanical sound)
    confirm.wav              — from Interface/bing05.wav
  doors/
    door_open.wav            — from Doors/automaticdoor01_open.wav
    door_close.wav           — from Doors/automaticdoor01_close.wav
  voice/
    engaging_warp.wav        — from Computervoices/voice01/engagingwarpdrive.wav
    all_systems_ready.wav    — from Computervoices/voice01/allsystemsarefunctioningperfectly.wav
    energy_low.wav           — from Computervoices/voice01/energyrunninglowsoon.wav
    danger.wav               — from Computervoices/voice01/danger.wav
    error.wav                — from Computervoices/voice01/error.wav
    engines_igniting.wav     — from Computervoices/voice01/enginesignitingstandbyforlaunch.wav
    alert.wav                — from Computervoices/voice01/alert.wav
    systems_online.wav       — from Computervoices/voice01/allsystemsreadyforliftoff.wav
  warp/
    spool.wav                — from Movements/Transform01.wav
    disengage.wav            — from Movements/Transform02.wav
    flash.wav                — from Movements/Movement01.wav (or similar impact)
  alarms/
    fuel_low.wav             — from Alarms/alarm01.wav
    fuel_critical.wav        — from Alarms/alarm06.wav
    power_failure.wav        — from Alarms/alarm10 loop.wav
  music/
    (all 30 tracks copied as-is, filenames preserved)
```

Exact file selections may be adjusted during implementation after auditioning.

---

## 9. Public API

```rust
pub struct AudioManager { ... }

impl AudioManager {
    /// Initialize audio system, load sound catalog.
    pub fn new(sounds_path: &str) -> Self;

    /// Update each frame: advance crossfades, process voice queue,
    /// trigger random creaks, manage music gaps.
    pub fn update(&mut self, dt: f32);

    /// Set listener position and orientation (from camera).
    pub fn set_listener(&mut self, pos: Vec3, forward: Vec3, up: Vec3);

    /// Set engine sound state. Crossfades between engine loops.
    pub fn set_engine_state(&mut self, drive_mode: DriveMode, throttle: f32, engine_on: bool);

    /// Set music context. Crossfades to appropriate playlist.
    pub fn set_music_context(&mut self, context: MusicContext);

    /// Set ship power state (affects ambience).
    pub fn set_power(&mut self, power_on: bool);

    /// Play a one-shot SFX at a world position.
    pub fn play_sfx(&mut self, id: SfxId, position: Vec3);

    /// Play a one-shot SFX with no spatial position (ship-wide).
    pub fn play_sfx_global(&mut self, id: SfxId);

    /// Queue a computer voice announcement.
    pub fn announce(&mut self, id: VoiceId);

    /// Start a looping alarm.
    pub fn play_alarm(&mut self, id: AlarmId);

    /// Stop a looping alarm.
    pub fn clear_alarm(&mut self, id: AlarmId);

    /// Set master volume (0.0 to 1.0).
    pub fn set_master_volume(&mut self, volume: f32);
}

pub enum MusicContext { Idle, Exploration, Warp, Tension, Discovery }
pub enum SfxId { ButtonClick, ButtonToggle, LeverMove, Confirm, DoorOpen, DoorClose }
pub enum VoiceId { EngagingWarp, AllSystemsReady, EnergyLow, Danger, Error, EnginesIgniting, Alert, SystemsOnline }
pub enum AlarmId { FuelLow, FuelCritical, PowerFailure }
```

---

## 10. Integration with Game Loop

In `spaceaway/src/main.rs`:

```rust
// Initialize (in setup)
let audio = sa_audio::AudioManager::new("resources/sounds/");

// Each frame (in RedrawRequested)
audio.set_listener(camera_pos, camera_forward, camera_up);
audio.set_engine_state(drive.mode(), ship.throttle, ship.engine_on);
audio.update(dt);

// On events
// Button click → audio.play_sfx(SfxId::ButtonClick, button_world_pos);
// Warp engage → audio.announce(VoiceId::EngagingWarp);
// Fuel low → audio.play_alarm(AlarmId::FuelLow);
// Enter system → audio.set_music_context(MusicContext::Discovery);
```

---

## 11. Future Enhancements (documented, not built now)

- **Voice selection**: player picks voice01-07 in settings
- **Ship jukebox**: physical music player in the ship, diegetic music
- **Occlusion**: sounds through bulkheads are muffled
- **Reverb**: different reverb per room (cockpit vs engine room vs corridor)
- **Footstep sounds**: player walking sounds on different surfaces
- **Atmosphere entry rumble**: sound when entering a planet's atmosphere
- **Combat sounds**: weapons, shields, hull impacts (when combat is added)
- **Multiplayer voice**: crew communication (Phase 6)
