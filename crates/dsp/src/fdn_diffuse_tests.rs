// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn validates_settings_and_bounds_all_history() {
    for bad in [f64::NAN, f64::INFINITY, -1., 101.] {
        assert!(DiffusedFdn::new(DiffuseSettings { predelay_ms: bad, ..DiffuseSettings::default() }).is_err());
    }
    for bad in [f32::NAN, f32::INFINITY, -0.1, 3.] {
        for settings in [
            DiffuseSettings { diffusion: bad, ..DiffuseSettings::default() },
            DiffuseSettings { early_level: bad, ..DiffuseSettings::default() },
            DiffuseSettings { width: bad, ..DiffuseSettings::default() },
        ] {
            assert!(DiffusedFdn::new(settings).is_err());
        }
    }
    let largest = DiffusedFdn::new(DiffuseSettings {
        core: Settings { sample_rate: 96_000, room_scale: 2., ..Settings::default() },
        predelay_ms: 100.,
        ..DiffuseSettings::default()
    })
    .unwrap();
    assert!(largest.history_bytes() < 1024 * 1024);
}

#[test]
fn processing_is_block_independent_and_reset_clears_every_path() {
    let mut whole = DiffusedFdn::new(DiffuseSettings::default()).unwrap();
    let mut split = DiffusedFdn::new(DiffuseSettings::default()).unwrap();
    let mut samples = vec![0.; 48_000];
    samples[..2].fill(0.25);
    let mut blocks = samples.clone();
    whole.process(&mut samples);
    for block in blocks.chunks_mut(74) {
        split.process(block);
    }
    assert_eq!(samples, blocks);
    whole.reset();
    assert!((0..48_000).all(|_| whole.process_frame([0.; 2]) == [0.; 2]));
    let mut odd = [0., 0., 42.];
    whole.process(&mut odd);
    assert_eq!(odd[2], 42.);
}

#[test]
fn early_path_starts_at_predelay_and_diffusion_reduces_impulse_concentration() {
    let mut core = Fdn::new(Settings::default()).unwrap();
    let mut developed = DiffusedFdn::new(DiffuseSettings::default()).unwrap();
    let mut old_energy = Vec::new();
    let mut new_energy = Vec::new();
    for i in 0..9600 {
        let x = if i == 0 { [0.25; 2] } else { [0.; 2] };
        old_energy.push(core.process_frame(x).iter().map(|v| v * v).sum::<f32>());
        new_energy.push(developed.process_frame(x).iter().map(|v| v * v).sum::<f32>());
    }
    assert_eq!(new_energy.iter().position(|v| *v > 1e-12), Some(120));
    let concentration = |v: &[f32]| v.iter().copied().fold(0_f32, f32::max) / v.iter().sum::<f32>();
    assert!(concentration(&new_energy) < concentration(&old_energy) * 0.6);
}

#[test]
fn invalid_samples_do_not_poison_diffusers_or_filters() {
    let mut core = DiffusedFdn::new(DiffuseSettings::default()).unwrap();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
        core.process_frame([value; 2]);
    }
    assert!((0..48_000).all(|_| core.process_frame([0.; 2]).iter().all(|v| v.is_finite() && v.abs() < 100.)));
}

#[test]
fn zero_width_folds_all_paths_to_mono() {
    let mut core = DiffusedFdn::new(DiffuseSettings { width: 0., ..DiffuseSettings::default() }).unwrap();
    for i in 0..24_000 {
        let y = core.process_frame(if i == 0 { [0.25; 2] } else { [0.; 2] });
        assert_eq!(y[0], y[1]);
    }
}
