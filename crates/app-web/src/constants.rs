// Frame smoothing and interaction tuning constants

// Exponential decay rate for internal pulse energy
pub const PULSE_ENERGY_DECAY_PER_SEC: f32 = 1.6;

// Target smoothing time constants (seconds)
pub const PULSE_RISE_TAU_SEC: f32 = 0.10;
pub const PULSE_FALL_TAU_SEC: f32 = 0.45;

// Pointer speed clamp (normalized units per second)
pub const POINTER_SPEED_MAX: f32 = 10.0;

// Inertial swirl spring parameters
pub const SWIRL_OMEGA: f32 = 1.1; // natural frequency
pub const SWIRL_DAMPING_RATIO: f32 = 0.5; // 0..1 critical at 1
pub const SWIRL_MAX_STEP_PER_SEC: f32 = 0.50; // cap motion per second (in uv units)

// Global FX mapping weights
pub const FX_REVERB_BASE: f32 = 0.35;
pub const FX_REVERB_SPAN: f32 = 0.65;

pub const FX_DELAY_WET_BASE: f32 = 0.15;
pub const FX_DELAY_WET_SWIRL: f32 = 0.55;
pub const FX_DELAY_WET_ECHO: f32 = 0.30;

pub const FX_DELAY_FB_BASE: f32 = 0.35;
pub const FX_DELAY_FB_SWIRL: f32 = 0.35;
pub const FX_DELAY_FB_ECHO: f32 = 0.25;

pub const FX_SAT_DRIVE_MIN: f32 = 0.2;
pub const FX_SAT_DRIVE_MAX: f32 = 3.0;
pub const FX_SAT_WET_BASE: f32 = 0.15;
pub const FX_SAT_WET_SPAN: f32 = 0.85;


