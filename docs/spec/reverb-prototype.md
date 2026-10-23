---
description: An experimental eight-line reverb core, reproducible listening files, acoustic analysis and CPU measurements
read_when: FDN prototype, reverb research, CPU budget, reverb measurement, listening renders
type: spec
status: planned
last_verified: 2026-10-23
covers:
  - crates/dsp/src/fdn.rs
  - crates/dsp/src/fdn_tests.rs
  - crates/dsp/examples/fdn_probe.rs
  - scripts/reverb/**
---

# Reverb prototype

The first research slice of [#897](https://github.com/suhina-daw/suhina-daw/issues/897)
is an experimental eight-line feedback delay network (FDN). It is a reusable DSP block
and a developer probe, not a registered device or an accepted production sound. The
existing `reverb` and its import mapping are unchanged. This probe processes the same
block that a future native device would use; integration through the engine, commands,
automation and the SDK panel remains to be done before the effect can ship.

## Signal and controls

Eight fixed integer delays feed a normalized Hadamard matrix followed by a cyclic
permutation. This transformation preserves energy. Each line's feedback gain is
`10^(-3 * delay_seconds / decay_seconds)`. An optional first-order high shelf makes
the high-frequency asymptotic decay a fraction of the low-frequency decay, with
delay-proportional loss. This simple baseline does not implement the two-stage
attenuation design from the references yet.
The shared shelf limits its per-trip gain to at least 0.001; extreme combinations
can therefore miss the requested high-decay ratio. The ratio is an experimental
target, not a guaranteed bandwise decay time; validate the resulting response.

The independent reference delays are 1429, 1777, 2081, 2383, 2719, 3083, 3469 and
3911 frames at 48 kHz, scaled to the sample rate and room scale. They are exploratory
choices, not extracted from a product or claimed to be perceptually optimal. Mono
excitation is equally distributed; two orthogonal output sign vectors provide
stereo. No early reflections, input diffusion or modulation are present initially.
Do not infer a production quality claim from a flat spectrum or an orthogonal matrix.

Construction accepts sample rates 8 through 384 kHz, decay 0.2 through 20 seconds,
room scale 0.5 through 2 and high-decay ratio 0.1 through 1. Invalid and non-finite
settings return errors before allocation. Construction allocates bounded delay
history; processing and reset do not allocate. Non-finite input becomes silence,
finite input is limited to +/-4, and tiny internal states are flushed to zero.
Output is wet-only stereo. The mono sum cancels opposite-polarity input; stereo
input preservation is outside this baseline. Odd interleaved trailing samples
are ignored. The core has no live parameter setters: reconstruct it off-thread.

## Probe and files

Run from a source checkout, with a release Rust toolchain:

```powershell
cargo run --release -p suhina-dsp --example fdn_probe -- target/reverb-prototype
python scripts/reverb/analyze.py target/reverb-prototype --output target/reverb-prototype/analysis.json
```

Use a Python environment with `pip install -r scripts/reverb/requirements.txt`.
Its NumPy and SciPy versions are recorded in the report. The JSON report has a CSV
companion. Existing report/output paths are refused rather than overwritten.

For a recorder capture of repeated impulses 20 seconds apart:

```powershell
python scripts/reverb/analyze.py capture.wav --confirm-impulse-response --period-seconds 20 --output capture-analysis.json
```

The first response onset is detected from the signal, or given with
`--first-onset-seconds`. Periodic splitting assumes constant spacing and no time
stretching. A partial final period is retained if it has at least half a second;
the decay validity check can reject its insufficient tail. An optional
`--baseline reference-impulse.wav` adds descriptive decay ratios to each result.
It expects a single impulse response, not a music file or a whole repeated capture.
These ratios are comparisons, not perceptual quality scores.
For a directory of external impulse WAVs, give the directory with
`--confirm-impulse-response`; this explicitly treats every WAV there as an IR.
The report includes octave-band energy in fixed tail windows, referenced to the
detected response onset. The filtering is causal; low-band ringing can affect
very short decay estimates and requires cross-checking before acceptance.

The probe creates original deterministic impulse, decaying noise percussion and
harmonic-tone stimuli, and wet-only and fixed dry/wet listening WAVs for a short
room, a two-second baseline and a long tail. These synthetic signals are not a
replacement for a maintainer listening to voice and recorded instruments. WAVs
are 32-bit float, stereo, at 48 kHz, without peak normalization. Stimulus amplitude,
settings, history bytes, build identity and analysis settings accompany results.
Listen quietly: wet levels differ, and listening files are not loudness matched.

For a controlled decay sweep, append `--flat-decay` to the probe command and use
a fresh output directory. This keeps room scale at 1 and high-decay ratio at 1
for all three decay targets (0.5, 2 and 6 seconds). Only decay time changes;
the ordinary listening presets above remain available. Compare wet-only responses
to captures made with frequency-dependent damping disabled. Matching numerical
controls does not establish equivalent algorithms or perceptual quality.

The analyzer also accepts a stereo or mono WAV path, including an impulse captured
by a recorder after an effect in a DAW. Explicitly select IR analysis only for an
actual impulse response: arbitrary music cannot establish the decay time of the
effect. In directory mode only named impulse responses are analyzed. It reports
Schroeder-integrated EDT/T20/T30 with fit quality and dynamic-range checks, bandwise
decay, stereo correlation and early echo-density windows. Invalid fits are null
with reasons. A truncated tail or noise floor can invalidate the result; silence
and non-finite input never produce a successful fit. This is not ISO certification.

The analysis environment needs NumPy and SciPy; these are development tools, not
Rust or application dependencies. Tests use analytical exponential decays, silent
and truncated responses and malformed input. REW/API cross-checking is a later
part of #897; the first probe must work without REW installed.

## Cost gate

CPU timing begins with the first runnable core. The probe reports repeated release
measurements for 48/96 kHz, 64/128-frame blocks and 1/8/16 independent instances,
with active input, silence and a decaying tail. Preparation, signal generation,
allocation and disk writes are outside timed spans. A warm-up precedes each trial.
Median, p95, p99 and observed maximum are for the whole group of instances per
block; percentages compare that work to one block's deadline on one core. Report
the build, OS and CPU alongside the JSON; shared-machine noise is not a guarantee
of worst-case execution time. Memory is allocated delay history, not process RSS.

Initial engineering targets on the development reference machine (Windows,
AMD Ryzen 5 5600X, release build) are: median aggregate throughput at most 1% of
one core per instance at 48 kHz, p99 group time below 50% of the block deadline
for 16 instances at 48 kHz/64 frames, and less than 1 MiB delay history per instance
at 96 kHz/scale 2. These are provisional gates to measure, not claimed results or
portable thresholds. Acoustic quality and low CPU are independent requirements.
Do not add lines or modulation without remeasuring both. Actual app playback,
underruns and parameter-movement cost require the later device integration.

## Sources

Original implementation of published methods; no upstream code or paper text copied.

- Heldmann and Schlecht (2021), *The Role of Modal Excitation in Colorless
  Reverberation*, DAFx: modal excitation balance matters alongside modal density.
- Dal Santo, Prawda, Schlecht and Välimäki (2025), *Optimizing tiny colorless
  feedback delay networks*, DOI: 10.1186/s13636-025-00401-w: tune spectral
  coloration and temporal density together; validate perceptually.
- Välimäki, Prawda and Schlecht (2024), *Two-Stage Attenuation Filter for Artificial
  Reverberation*, DOI: 10.1109/LSP.2024.3352510: candidate future damping design.
