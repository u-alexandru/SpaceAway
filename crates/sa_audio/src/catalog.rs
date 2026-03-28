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
