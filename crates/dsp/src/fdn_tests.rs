// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn mixing_preserves_energy_and_output_vectors_are_orthogonal() {
    for i in 0..8 {
        let mut v = [0.; 8];
        v[i] = 1.;
        mix(&mut v);
        assert!((v.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 1e-6);
    }
    assert_eq!(LEFT.iter().zip(RIGHT).map(|(a, b)| a * b).sum::<f32>(), 0.);
}

#[test]
fn rejects_hostile_settings_and_bounds_history() {
    for bad in [f64::NAN, f64::INFINITY, -1., 0., 1000.] {
        assert!(Fdn::new(Settings { decay_seconds: bad, ..Settings::default() }).is_err());
        assert!(Fdn::new(Settings { room_scale: bad, ..Settings::default() }).is_err());
        assert!(Fdn::new(Settings { high_decay_ratio: bad, ..Settings::default() }).is_err());
    }
    for rate in [0, 7999, 384_001, u32::MAX] {
        assert!(Fdn::new(Settings { sample_rate: rate, ..Settings::default() }).is_err());
    }
    let largest = Fdn::new(Settings { sample_rate: 384_000, room_scale: 2., ..Settings::default() }).unwrap();
    assert!(largest.history_bytes() <= 4 * 1024 * 1024);
}

#[test]
fn rendering_is_block_independent_and_reset_removes_tail() {
    let settings = Settings { high_decay_ratio: 0.4, ..Settings::default() };
    let mut a = Fdn::new(settings).unwrap();
    let mut b = Fdn::new(settings).unwrap();
    let mut signal = vec![0.; 48_000 * 2];
    signal[0] = 0.25;
    signal[1] = 0.25;
    let mut split = signal.clone();
    a.process(&mut signal);
    for block in split.chunks_mut(128) {
        b.process(block);
    }
    assert_eq!(signal, split);
    a.reset();
    assert!((0..10_000).all(|_| a.process_frame([0., 0.]) == [0., 0.]));
    let mut odd = [0., 0., 42.];
    a.process(&mut odd);
    assert_eq!(odd[2], 42.);
}

#[test]
fn hostile_input_does_not_poison_network() {
    let mut fdn = Fdn::new(Settings::default()).unwrap();
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, -f32::MAX] {
        fdn.process_frame([x, x]);
    }
    assert!((0..48_000).all(|_| fdn.process_frame([0., 0.]).iter().all(|x| x.is_finite() && x.abs() < 100.)));
}

#[test]
fn decay_tracks_target_at_multiple_sample_rates() {
    for rate in [8000, 44_100, 48_000, 96_000] {
        for decay in [0.5, 2.] {
            let mut fdn = Fdn::new(Settings {
                sample_rate: rate,
                decay_seconds: decay,
                damping_hz: 2000.,
                ..Settings::default()
            })
            .unwrap();
            let mut energy = vec![0.; (f64::from(rate) * decay * 2.) as usize];
            for (i, e) in energy.iter_mut().enumerate() {
                let input = if i == 0 { [0.25; 2] } else { [0.; 2] };
                let out = fdn.process_frame(input);
                *e = out.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
            }
            let mut sum = 0.;
            for e in energy.iter_mut().rev() {
                sum += *e;
                *e = sum;
            }
            let total = energy[0];
            let first = energy.iter().position(|e| *e / total < 10_f64.powf(-0.5)).unwrap();
            let last = energy.iter().position(|e| *e / total < 10_f64.powf(-3.5)).unwrap();
            let measured = 2. * (last - first) as f64 / f64::from(rate);
            assert!((measured / decay - 1.).abs() < 0.12, "rate={rate} decay={decay} measured={measured}");
        }
    }
}
