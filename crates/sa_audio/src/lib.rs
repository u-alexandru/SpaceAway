//! sa_audio: spatial audio engine for SpaceAway.

pub mod catalog;
pub mod channel;
pub mod spatial;

pub use catalog::{AlarmId, EngineState, MusicContext, SfxId, VoiceId};
pub use channel::Channels;
pub use spatial::Listener;

use std::path::{Path, PathBuf};

/// Top-level audio manager — owns the channel mixer and exposes the game API.
pub struct AudioManager {
    channels: Channels,
    sounds_root: PathBuf,
    listener: Listener,
    master_volume: f32,
}

impl AudioManager {
    /// Create a new audio manager. `sounds_root` is the directory containing
    /// all sound sub-folders (engine/, music/, ambience/, etc.).
    pub fn new(sounds_root: PathBuf) -> Self {
        Self {
            channels: Channels::new(),
            sounds_root,
            listener: Listener::default(),
            master_volume: 1.0,
        }
    }

    /// Call once per frame with the elapsed time in seconds.
    pub fn update(&mut self, dt: f32) {
        self.channels
            .update(dt, &self.sounds_root, &self.listener, self.master_volume);
    }

    /// Update the listener (camera) position and orientation.
    pub fn set_listener(&mut self, listener: Listener) {
        self.listener = listener;
    }

    /// Set the master volume (0.0 .. 1.0).
    pub fn set_master_volume(&mut self, vol: f32) {
        self.master_volume = vol.clamp(0.0, 1.0);
    }

    /// Transition the engine sound to a new state.
    pub fn set_engine_state(&mut self, state: EngineState) {
        self.channels.set_engine_state(state, &self.sounds_root);
    }

    /// Toggle ship power (ambient hum, life-support).
    pub fn set_power(&mut self, on: bool) {
        self.channels.set_power(on, &self.sounds_root);
    }

    /// Switch the music playlist context.
    pub fn set_music_context(&mut self, ctx: MusicContext) {
        self.channels.set_music_context(ctx);
    }

    /// Play a one-shot SFX, optionally spatialised at `position`.
    pub fn play_sfx(&mut self, id: SfxId, position: Option<glam::Vec3>) {
        self.channels
            .play_sfx(id, position, &self.sounds_root, &self.listener);
    }

    /// Queue a voice announcement.
    pub fn announce(&mut self, id: VoiceId) {
        self.channels.announce(id, &self.sounds_root);
    }

    /// Start a looping alarm.
    pub fn play_alarm(&mut self, id: AlarmId) {
        self.channels.play_alarm(id, &self.sounds_root);
    }

    /// Stop a specific alarm.
    pub fn clear_alarm(&mut self, id: AlarmId) {
        self.channels.clear_alarm(id);
    }

    /// Reference to the sounds root directory.
    pub fn sounds_root(&self) -> &Path {
        &self.sounds_root
    }
}
