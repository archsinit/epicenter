//! Pure signal calculation shared by the native recorder and its tests.
pub fn rms(samples: impl Iterator<Item = f32>) -> f32 {
    let (sum, count) = samples.fold((0.0_f64, 0_u64), |(sum, count), value| {
        let sample = if value.is_finite() {
            value.clamp(-1.0, 1.0) as f64
        } else {
            0.0
        };
        (sum + sample * sample, count + 1)
    });
    if count == 0 {
        0.0
    } else {
        (sum / count as f64).sqrt() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::rms;
    #[test]
    fn silence_and_empty_input_do_not_make_a_waveform() {
        assert_eq!(rms([0.0_f32; 128].into_iter()), 0.0);
        assert_eq!(rms(std::iter::empty()), 0.0);
    }
    #[test]
    fn amplitude_comes_from_actual_samples() {
        assert!((rms([0.5_f32, -0.5].into_iter()) - 0.5).abs() < 0.00001);
        assert!(rms([0.1_f32; 16].into_iter()) < rms([0.8_f32; 16].into_iter()));
    }
    #[test]
    fn invalid_and_clipped_samples_stay_bounded() {
        assert_eq!(rms([f32::NAN, f32::INFINITY].into_iter()), 0.0);
        assert_eq!(rms([2.0_f32, -2.0].into_iter()), 1.0);
    }
}
