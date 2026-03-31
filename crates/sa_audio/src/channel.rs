//! Audio channel management: engine, ambience, music, voice, alarms.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use rand::Rng;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};

use crate::catalog::*;
use crate::spatial::{spatial_params, Listener};

/// Manages all audio playback channels via rodio.
pub struct Channels {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    // Engine
    engine_sink: Option<Sink>,
    engine_state: EngineState,
    // Ambience
    ambience_hum: Option<Sink>,
    ambience_life: Option<Sink>,
    power_on: bool,
    creak_timer: f32,
    // Music
    music_sink: Option<Sink>,
    music_fading_out: Option<Sink>, // old track fading out during crossfade
    music_context: MusicContext,
    music_gap_timer: f32,
    music_playing: bool,
    music_volume: f32,        // current fade level 0.0-1.0
    music_target_volume: f32, // target fade level
    music_base_volume: f32,   // configured volume (0.4)
    // Voice
    voice_sink: Option<Sink>,
    voice_queue: Vec<VoiceId>,
    // Alarms
    alarm_sinks: HashMap<u8, Sink>,
}

impl Default for Channels {
    fn default() -> Self {
        Self::new()
    }
}

impl Channels {
    /// Open the default audio output device and initialise all channels.
    pub fn new() -> Self {
        let (stream, handle) =
            OutputStream::try_default().expect("Failed to open audio output");
        Self {
            _stream: stream,
            stream_handle: handle,
            engine_sink: None,
            engine_state: EngineState::Off,
            ambience_hum: None,
            ambience_life: None,
            power_on: false,
            creak_timer: 60.0,
            music_sink: None,
            music_fading_out: None,
            music_context: MusicContext::Idle,
            music_gap_timer: 5.0,
            music_playing: false,
            music_volume: 0.0,
            music_target_volume: 1.0,
            music_base_volume: 0.2,
            voice_sink: None,
            voice_queue: Vec::new(),
            alarm_sinks: HashMap::new(),
        }
    }

    /// Per-frame tick: advance voice queue, music gaps, random creaks.
    pub fn update(
        &mut self,
        dt: f32,
        sounds_root: &Path,
        _listener: &Listener,
        master_volume: f32,
    ) {
        self.update_voice(sounds_root);
        self.update_music(dt, sounds_root, master_volume);
        self.update_creaks(dt, sounds_root);
    }

    // -- Engine ---------------------------------------------------------------

    /// Transition the engine sound to a new state.
    pub fn set_engine_state(&mut self, state: EngineState, sounds_root: &Path) {
        if state == self.engine_state {
            return;
        }
        self.engine_state = state;
        if let Some(sink) = self.engine_sink.take() {
            sink.stop();
        }
        if let Some(rel) = engine_path(state) {
            let full = sounds_root.join(rel);
            if let Ok(sink) = self.load_looping(&full) {
                let vol = match state {
                    EngineState::Off => 0.0,
                    EngineState::Idle => 0.15,
                    EngineState::Impulse => 0.3,
                    EngineState::Cruise => 0.4,
                    EngineState::WarpSpool => 0.5,
                    EngineState::WarpEngaged => 0.6,
                };
                sink.set_volume(vol);
                self.engine_sink = Some(sink);
            }
        }
    }

    // -- Ambience -------------------------------------------------------------

    /// Toggle ship power (controls ambient hum / life-support loops).
    pub fn set_power(&mut self, on: bool, sounds_root: &Path) {
        if on == self.power_on {
            return;
        }
        self.power_on = on;
        if on {
            self.start_ambience(sounds_root);
        } else {
            self.stop_ambience();
        }
    }

    // -- Music ----------------------------------------------------------------

    /// Switch the music playlist context.
    pub fn set_music_context(&mut self, ctx: MusicContext) {
        if ctx == self.music_context {
            return;
        }
        self.music_context = ctx;
        // Move current track to fading_out (will be faded by update_music)
        if let Some(old) = self.music_fading_out.take() {
            old.stop(); // stop any previous fading track
        }
        if let Some(current) = self.music_sink.take() {
            self.music_fading_out = Some(current);
        }
        self.music_gap_timer = 0.5; // brief gap — new track starts almost immediately
        self.music_playing = false;
        self.music_volume = 0.0; // new track will fade in
        self.music_target_volume = 1.0;
    }

    // -- SFX ------------------------------------------------------------------

    /// Play a one-shot sound effect, optionally spatialised.
    pub fn play_sfx(
        &mut self,
        id: SfxId,
        position: Option<glam::Vec3>,
        sounds_root: &Path,
        listener: &Listener,
    ) {
        let path = sounds_root.join(sfx_path(id));
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("SFX file missing: {path:?}: {e}");
                return;
            }
        };
        let source = match Decoder::new(BufReader::new(file)) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("SFX decode error: {path:?}: {e}");
                return;
            }
        };
        if let Ok(sink) = Sink::try_new(&self.stream_handle) {
            let vol = if let Some(pos) = position {
                let (v, _pan) = spatial_params(listener, pos, 30.0);
                v * 0.7
            } else {
                0.7
            };
            sink.set_volume(vol);
            sink.append(source);
            sink.detach();
        }
    }

    // -- Voice ----------------------------------------------------------------

    /// Queue a voice announcement (higher priority can interrupt).
    pub fn announce(&mut self, id: VoiceId, sounds_root: &Path) {
        let playing = self.voice_sink.as_ref().is_some_and(|s| !s.empty());
        if !playing {
            self.play_voice(id, sounds_root);
        } else {
            self.voice_queue.push(id);
            self.voice_queue.sort_by_key(|v| Reverse(v.priority()));
        }
    }

    // -- Alarms ---------------------------------------------------------------

    /// Start a looping alarm (no-op if already playing).
    pub fn play_alarm(&mut self, id: AlarmId, sounds_root: &Path) {
        let key = id as u8;
        if self.alarm_sinks.contains_key(&key) {
            return;
        }
        let path = sounds_root.join(alarm_path(id));
        if let Ok(sink) = self.load_looping(&path) {
            sink.set_volume(0.3);
            self.alarm_sinks.insert(key, sink);
        }
    }

    /// Stop a specific alarm.
    pub fn clear_alarm(&mut self, id: AlarmId) {
        let key = id as u8;
        if let Some(sink) = self.alarm_sinks.remove(&key) {
            sink.stop();
        }
    }

    // -- Internal helpers -----------------------------------------------------

    fn load_looping(&self, path: &Path) -> Result<Sink, ()> {
        let file = File::open(path).map_err(|e| {
            log::warn!("Audio file not found: {path:?}: {e}");
        })?;
        let source = Decoder::new(BufReader::new(file)).map_err(|e| {
            log::warn!("Decode error: {path:?}: {e}");
        })?;
        let sink = Sink::try_new(&self.stream_handle).map_err(|_| ())?;
        sink.append(source.repeat_infinite());
        Ok(sink)
    }

    fn play_voice(&mut self, id: VoiceId, sounds_root: &Path) {
        if let Some(old) = self.voice_sink.take() {
            old.stop();
        }
        let path = sounds_root.join(voice_path(id));
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Voice file missing: {path:?}: {e}");
                return;
            }
        };
        let source = match Decoder::new(BufReader::new(file)) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Voice decode error: {path:?}: {e}");
                return;
            }
        };
        if let Ok(sink) = Sink::try_new(&self.stream_handle) {
            sink.set_volume(0.8);
            sink.append(source);
            self.voice_sink = Some(sink);
            log::debug!("Voice: {:?}", id);
        }
    }

    fn start_ambience(&mut self, sounds_root: &Path) {
        // Very subtle ambience — felt more than heard.
        // Ship hum barely perceptible, life support even quieter.
        // Music carries the atmosphere, not ambient drones.
        if self.ambience_hum.is_none()
            && let Ok(sink) =
                self.load_looping(&sounds_root.join(ambience_hum_path()))
        {
            sink.set_volume(0.03); // barely perceptible
            self.ambience_hum = Some(sink);
        }
        if self.ambience_life.is_none()
            && let Ok(sink) =
                self.load_looping(&sounds_root.join(ambience_life_support_path()))
        {
            sink.set_volume(0.02); // even quieter
            self.ambience_life = Some(sink);
        }
    }

    fn stop_ambience(&mut self) {
        if let Some(s) = self.ambience_hum.take() {
            s.stop();
        }
        if let Some(s) = self.ambience_life.take() {
            s.stop();
        }
    }

    fn update_voice(&mut self, sounds_root: &Path) {
        let done = self.voice_sink.as_ref().is_none_or(|s| s.empty());
        if done && !self.voice_queue.is_empty() {
            let next = self.voice_queue.remove(0);
            self.play_voice(next, sounds_root);
        }
    }

    fn update_music(&mut self, dt: f32, sounds_root: &Path, master_volume: f32) {
        let fade_speed = 0.5; // 2-second fade (0→1 in 2s)

        // Fade out the old track (from context change)
        if let Some(ref old_sink) = self.music_fading_out {
            // Compute the old track's fade-out volume
            let old_vol = old_sink.volume();
            let new_vol = (old_vol - fade_speed * dt * self.music_base_volume).max(0.0);
            if new_vol <= 0.001 {
                if let Some(s) = self.music_fading_out.take() { s.stop(); }
            } else {
                old_sink.set_volume(new_vol);
            }
        }

        // Fade in the current track
        if self.music_playing {
            if self.music_volume < self.music_target_volume {
                self.music_volume = (self.music_volume + fade_speed * dt).min(self.music_target_volume);
                if let Some(ref sink) = self.music_sink {
                    sink.set_volume(self.music_volume * self.music_base_volume * master_volume);
                }
            }
            // Check if track finished
            if let Some(ref sink) = self.music_sink
                && sink.empty() {
                    self.music_playing = false;
                    self.music_gap_timer = rand::thread_rng().gen_range(30.0..90.0);
                }
        } else {
            // Not playing — count down gap timer, then start new track
            self.music_gap_timer -= dt;
            if self.music_gap_timer <= 0.0 {
                let tracks = music_tracks(self.music_context);
                if !tracks.is_empty() {
                    let idx = rand::thread_rng().gen_range(0..tracks.len());
                    let stem = tracks[idx];
                    if let Some(path) = crate::catalog::resolve_music_path(sounds_root, stem) {
                        match File::open(&path) {
                            Ok(file) => match Decoder::new(BufReader::new(file)) {
                                Ok(source) => {
                                    if let Ok(sink) = Sink::try_new(&self.stream_handle) {
                                        self.music_volume = 0.0; // start silent, fade in
                                        self.music_target_volume = 1.0;
                                        sink.set_volume(0.0);
                                        sink.append(source);
                                        self.music_sink = Some(sink);
                                        self.music_playing = true;
                                        log::info!("Music: {}", path.display());
                                    }
                                }
                                Err(e) => log::warn!("Music decode error {:?}: {}", path, e),
                            },
                            Err(e) => log::warn!("Music file not found {:?}: {}", path, e),
                        }
                    } else {
                        log::warn!("Music not found (tried .ogg/.wav): {stem}");
                    }
                }
            }
        }
    }

    fn update_creaks(&mut self, dt: f32, sounds_root: &Path) {
        if !self.power_on {
            return;
        }
        self.creak_timer -= dt;
        if self.creak_timer <= 0.0 {
            let paths = ambience_creak_paths();
            if !paths.is_empty() {
                let mut rng = rand::thread_rng();
                let idx = rng.gen_range(0..paths.len());
                let path = sounds_root.join(paths[idx]);
                if let Ok(file) = File::open(&path)
                    && let Ok(source) = Decoder::new(BufReader::new(file))
                    && let Ok(sink) = Sink::try_new(&self.stream_handle)
                {
                    sink.set_volume(rng.gen_range(0.03..0.08));
                    sink.append(source);
                    sink.detach();
                }
            }
            self.creak_timer = rand::thread_rng().gen_range(30.0..120.0);
        }
    }
}
