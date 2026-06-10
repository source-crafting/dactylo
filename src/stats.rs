pub fn net_wpm(correct_chars: usize, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    (correct_chars as f64 / 5.0) / (seconds / 60.0)
}

pub fn raw_wpm(typed_chars: usize, seconds: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    (typed_chars as f64 / 5.0) / (seconds / 60.0)
}

pub fn accuracy(correct_keystrokes: usize, total_keystrokes: usize) -> f64 {
    if total_keystrokes == 0 {
        return 100.0;
    }
    correct_keystrokes as f64 / total_keystrokes as f64 * 100.0
}

pub fn consistency(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    if mean <= 0.0 {
        return 0.0;
    }
    let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / samples.len() as f64;
    let cv = variance.sqrt() / mean * 100.0;
    (100.0 - cv).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_wpm_basic() {
        assert!((net_wpm(250, 60.0) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn wpm_zero_seconds_is_zero() {
        assert_eq!(net_wpm(100, 0.0), 0.0);
        assert_eq!(raw_wpm(100, 0.0), 0.0);
    }

    #[test]
    fn raw_wpm_counts_all_chars() {
        assert!((raw_wpm(300, 60.0) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn accuracy_basic() {
        assert!((accuracy(95, 100) - 95.0).abs() < 1e-9);
    }

    #[test]
    fn accuracy_zero_typed_is_hundred() {
        assert_eq!(accuracy(0, 0), 100.0);
    }

    #[test]
    fn accuracy_all_wrong_is_zero() {
        assert_eq!(accuracy(0, 50), 0.0);
    }

    #[test]
    fn consistency_constant_samples_is_hundred() {
        assert_eq!(consistency(&[50.0, 50.0, 50.0]), 100.0);
    }

    #[test]
    fn consistency_empty_is_zero() {
        assert_eq!(consistency(&[]), 0.0);
    }

    #[test]
    fn consistency_varied_is_between() {
        // mean 50, sd 10, cv 20% -> 80
        let c = consistency(&[40.0, 60.0]);
        assert!((c - 80.0).abs() < 1e-9);
    }

    #[test]
    fn consistency_clamped_at_zero() {
        // mean 50, sd 50, cv 100% -> 0
        assert_eq!(consistency(&[0.0, 100.0]), 0.0);
    }
}
