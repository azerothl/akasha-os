//! Pentatonic score timed to the beat sheet. PCM WAV, no audio crate.

use aos_proto::IllustrationTimelineBeat;

const SAMPLE_RATE: u32 = 22050;

pub fn score_wav(beats: &[IllustrationTimelineBeat]) -> Vec<u8> {
    let total = beats.iter().map(|b| b.dur_s.max(0.05)).sum::<f32>().max(0.5);
    let n = (total * SAMPLE_RATE as f32) as usize + SAMPLE_RATE as usize / 10;
    let mut samples = vec![0.0f32; n];
    let mut t0 = 0.0f32;
    for (i, beat) in beats.iter().enumerate() {
        let hz = pentatonic(i);
        let dur = 0.22f32.min(beat.dur_s * 0.6);
        add_note(&mut samples, t0, dur, hz, 0.18);
        if beat.name == "signoff" {
            add_note(&mut samples, t0, 0.9, pentatonic(i + 2), 0.1);
        }
        t0 += beat.dur_s.max(0.05);
    }
    encode_wav(&samples)
}

fn pentatonic(step: usize) -> f32 {
    const HZ: [f32; 5] = [261.63, 293.66, 329.63, 392.0, 440.0];
    HZ[step % 5] * if step > 4 { 2.0 } else { 1.0 }
}

fn add_note(samples: &mut [f32], t0: f32, dur: f32, hz: f32, gain: f32) {
    let start = (t0 * SAMPLE_RATE as f32) as usize;
    let len = (dur * SAMPLE_RATE as f32) as usize;
    for i in 0..len {
        let idx = start + i;
        if idx >= samples.len() {
            break;
        }
        let env = 1.0 - i as f32 / len as f32;
        let phase = i as f32 / SAMPLE_RATE as f32 * hz * std::f32::consts::TAU;
        samples[idx] += phase.sin() * gain * env;
    }
}

fn encode_wav(samples: &[f32]) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(&pcm);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::IllustrationTimelineBeat;

    #[test]
    fn wav_header_and_body() {
        let beats = vec![IllustrationTimelineBeat {
            name: "airborne".into(),
            dur_s: 1.0,
            pose: Default::default(),
            camera: None,
            mode: None,
        }];
        let wav = score_wav(&beats);
        assert!(wav.starts_with(b"RIFF"));
        assert!(wav.len() > 44);
    }
}
