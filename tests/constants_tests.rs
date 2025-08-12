// Host-side tests for constants and their mathematical relationships.
// The main crate is wasm-only, so we include the pure-Rust modules directly.

#![allow(dead_code)]
mod constants {
    include!("../src/constants.rs");
}
mod core_constants {
    include!("../src/core/constants.rs");
}

use constants::*;
use core_constants::*;

#[test]
#[allow(clippy::assertions_on_constants)]
fn constants_are_within_reasonable_bounds() {
    // Time constants should be positive
    assert!(PULSE_ENERGY_DECAY_PER_SEC > 0.0);
    assert!(PULSE_RISE_TAU_SEC > 0.0);
    assert!(PULSE_FALL_TAU_SEC > 0.0);

    // Speed limits should be positive
    assert!(POINTER_SPEED_MAX > 0.0);
    assert!(SWIRL_MAX_STEP_PER_SEC > 0.0);

    // Damping ratio should be between 0 and 1
    assert!(SWIRL_DAMPING_RATIO >= 0.0 && SWIRL_DAMPING_RATIO <= 1.0);

    // Weights should be between 0 and 1
    assert!(SWIRL_TARGET_WEIGHT_POINTER >= 0.0 && SWIRL_TARGET_WEIGHT_POINTER <= 1.0);
    assert!(SWIRL_TARGET_WEIGHT_VELOCITY >= 0.0 && SWIRL_TARGET_WEIGHT_VELOCITY <= 1.0);
    assert!(SWIRL_TARGET_CLICK_BONUS >= 0.0 && SWIRL_TARGET_CLICK_BONUS <= 1.0);
    assert!(SWIRL_ENERGY_BLEND_ALPHA >= 0.0 && SWIRL_ENERGY_BLEND_ALPHA <= 1.0);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn fx_weights_sum_to_reasonable_values() {
    // Reverb weights
    assert!(FX_REVERB_BASE + FX_REVERB_SPAN <= 1.0);

    // Delay wet weights
    assert!(FX_DELAY_WET_BASE + FX_DELAY_WET_SWIRL + FX_DELAY_WET_ECHO <= 1.0);

    // Delay feedback weights
    assert!(FX_DELAY_FB_BASE + FX_DELAY_FB_SWIRL + FX_DELAY_FB_ECHO <= 1.0);

    // Saturation weights
    assert!(FX_SAT_WET_BASE + FX_SAT_WET_SPAN <= 1.0);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn core_constants_are_positive() {
    assert!(SPREAD > 0.0);
    assert!(BASE_SCALE > 0.0);
    assert!(SCALE_PULSE_MULTIPLIER > 0.0);
    assert!(PICK_SPHERE_RADIUS > 0.0);
    assert!(ENGINE_DRAG_MAX_RADIUS > 0.0);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn constants_have_logical_relationships() {
    // Pulse fall should be slower than rise (more time constant)
    assert!(PULSE_FALL_TAU_SEC > PULSE_RISE_TAU_SEC);

    // Engine drag radius should be larger than pick sphere radius
    assert!(ENGINE_DRAG_MAX_RADIUS > PICK_SPHERE_RADIUS);

    // Saturation drive range should be positive
    assert!(FX_SAT_DRIVE_MAX > FX_SAT_DRIVE_MIN);
    assert!(FX_SAT_DRIVE_MIN > 0.0);

    // Level mapping should be reasonable
    assert!(LEVEL_BASE + LEVEL_SPAN <= 1.0);
    assert!(LEVEL_BASE > 0.0);
}
