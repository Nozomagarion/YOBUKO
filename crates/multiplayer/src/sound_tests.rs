// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use std::io::Cursor;

fn hound_wave(data: &AudioData) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    let spec = hound::WavSpec {
        channels: data.channels,
        sample_rate: data.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::new(&mut out, spec).unwrap();
    for sample in &data.samples {
        writer.write_sample(*sample).unwrap();
    }
    writer.finalize().unwrap();
    out.into_inner()
}
fn encoded(data: &AudioData) -> (Vec<u8>, Asset, Vec<ChainingValue>) {
    let mut bytes = Vec::new();
    let (meta, cvs) = encode(data, &mut bytes, &AtomicBool::new(false)).unwrap();
    (bytes, meta, cvs)
}

#[test]
fn the_streamed_copy_is_byte_identical_to_the_canonical_float_wave() {
    for channels in [1, 2] {
        let data = AudioData {
            sample_rate: 44100,
            channels,
            samples: (0..9000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect(),
        };
        let (bytes, meta, _) = encoded(&data);
        assert_eq!(bytes, hound_wave(&data));
        assert_eq!(meta.hash, blake3::hash(&bytes).to_hex().to_string());
        assert_eq!(meta.bytes, bytes.len());
        assert_eq!(meta.frames, 9000 / usize::from(channels));
    }
}

#[test]
fn the_chunk_tree_reproduces_the_whole_file_hash_at_every_size() {
    let sizes = [1, 1023, 1024, 1025, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK, 3 * CHUNK + 5, 7 * CHUNK, 9 * CHUNK - 3];
    for size in sizes {
        let bytes: Vec<u8> = (0..size).map(|i| (i * 31 % 251) as u8).collect();
        let mut tree = Tree::new();
        // Uneven writes must not change the chunking.
        for part in bytes.chunks(777) {
            tree.update(part);
        }
        let (hash, cvs) = tree.finish();
        assert_eq!(hash, blake3::hash(&bytes).to_hex().to_string(), "size {size}");
        if size <= CHUNK {
            assert!(cvs.is_empty());
        } else {
            assert_eq!(cvs.len(), leaves(size));
            assert_eq!(root(&cvs, size).as_deref(), Some(hash.as_str()), "size {size}");
            for (i, part) in bytes.chunks(CHUNK).enumerate() {
                assert_eq!(leaf(i * CHUNK, part).unwrap(), cvs[i]);
            }
        }
    }
}

#[test]
fn forged_trees_and_altered_chunks_are_refused() {
    let data = AudioData { sample_rate: 8000, channels: 1, samples: (0..5000).map(|i| i as f32 * 1e-4).collect() };
    let (bytes, meta, cvs) = encoded(&data);
    assert!(cvs.len() > 3);
    verify_tree(&meta, &cvs).unwrap();
    let mut forged = cvs.clone();
    forged[1][0] ^= 1;
    assert!(verify_tree(&meta, &forged).is_err());
    assert!(verify_tree(&meta, &cvs[1..]).is_err());
    assert!(verify_tree(&meta, &[]).is_err());
    let chunk = &bytes[CHUNK..2 * CHUNK];
    verify_chunk(&meta, &cvs, CHUNK, chunk).unwrap();
    let mut altered = chunk.to_vec();
    altered[10] ^= 0x40;
    assert!(verify_chunk(&meta, &cvs, CHUNK, &altered).is_err());
    // A chunk is bound to its position, not only to its bytes.
    assert!(verify_chunk(&meta, &cvs, 2 * CHUNK, chunk).is_err());
    assert!(verify_chunk(&meta, &cvs, cvs.len() * CHUNK, chunk).is_err());
    // A one-chunk sound has no tree and is checked against its hash.
    let small = AudioData { sample_rate: 8000, channels: 1, samples: vec![0.25; 8] };
    let (bytes, meta, cvs) = encoded(&small);
    assert!(cvs.is_empty());
    verify_tree(&meta, &[]).unwrap();
    verify_chunk(&meta, &[], 0, &bytes).unwrap();
    assert!(verify_chunk(&meta, &[], 0, &bytes[..bytes.len() - 1]).is_err());
    assert!(verify_tree(&meta, &[[0; 32], [0; 32]]).is_err());
}

#[test]
fn decoding_from_disk_checks_header_samples_size_and_hash() {
    let dir = tempfile::tempdir().unwrap();
    let data = AudioData { sample_rate: 8000, channels: 2, samples: vec![0.1, -0.25, 0.3, 0.5, 0.0, 1.0] };
    let (bytes, meta, _) = encoded(&data);
    let path = dir.path().join("sound.wav");
    std::fs::write(&path, &bytes).unwrap();
    let stop = AtomicBool::new(false);
    assert_eq!(decode(&path, &meta, &stop).unwrap(), data);
    assert_eq!(tree_of(&path, &meta, &stop).unwrap(), Vec::<ChainingValue>::new());
    assert!(decode(&path, &meta, &AtomicBool::new(true)).is_err());
    let mut forged = meta.clone();
    forged.frames = usize::MAX;
    assert!(decode(&path, &forged, &stop).is_err());
    let mut forged = meta.clone();
    forged.rate = 44100;
    assert!(decode(&path, &forged, &stop).is_err(), "header must match the manifest");
    // A non-finite sample is refused even when the hash matches the altered bytes.
    let mut bad = bytes.clone();
    bad[HEADER..HEADER + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    std::fs::write(&path, &bad).unwrap();
    let mut rehashed = meta.clone();
    rehashed.hash = blake3::hash(&bad).to_hex().to_string();
    assert!(decode(&path, &rehashed, &stop).is_err());
    assert!(decode(&path, &meta, &stop).is_err(), "hash mismatch");
    let mut bad = bytes.clone();
    bad[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&path, &bad).unwrap();
    rehashed.hash = blake3::hash(&bad).to_hex().to_string();
    assert!(decode(&path, &rehashed, &stop).is_err(), "non-canonical header");
    std::fs::write(&path, &bytes[..bytes.len() - 4]).unwrap();
    assert!(decode(&path, &meta, &stop).is_err(), "truncated file");
}

#[test]
fn unsupported_audio_is_refused_before_writing() {
    let mut out = Vec::new();
    for data in [
        AudioData { sample_rate: 8000, channels: 3, samples: vec![0.0; 3] },
        AudioData { sample_rate: 7999, channels: 1, samples: vec![0.0] },
        AudioData { sample_rate: 8000, channels: 1, samples: vec![] },
        AudioData { sample_rate: 8000, channels: 1, samples: vec![f32::INFINITY] },
        AudioData { sample_rate: 8000, channels: 2, samples: vec![0.0; 3] },
    ] {
        assert!(encode(&data, &mut out, &AtomicBool::new(false)).is_err());
        assert!(out.is_empty());
    }
}
