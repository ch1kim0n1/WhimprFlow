//! Two-second microphone self-test returning peak RMS (0.0–1.0).

pub fn peak_rms_2s() -> Result<f32, String> {
    let handle =
        whimpr_audio::start(|_| {}).map_err(|e| format!("microphone capture failed: {e}"))?;
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let Some(result) = handle.stop() else {
        return Err("no audio samples captured; check the default input device".into());
    };
    let samples = result.samples;
    if samples.is_empty() {
        return Err("no audio samples captured; check the default input device".into());
    }
    let mut peak = 0.0f32;
    for &s in &samples {
        let a = s.abs();
        if a > peak {
            peak = a;
        }
    }
    let sum_sq: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt() as f32;
    Ok(peak.max(rms * 2.0).min(1.0))
}
