/// Abbreviates a number to a human-readable format.
pub fn abbreviate_num(num: u64) -> String {
    if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{}k", num / 1_000)
    } else {
        format!("{num}")
    }
}

pub fn mean(data: &[u64]) -> Option<f64> {
    let sum: f64 = data.iter().map(|d| *d as f64).sum();
    let count = data.len();

    match count {
        positive if positive > 0 => Some(sum / count as f64),
        _ => None,
    }
}

pub fn std_deviation(data: &[u64]) -> Option<f64> {
    match (mean(data), data.len()) {
        (Some(data_mean), count) if count > 0 => {
            let variance = data
                .iter()
                .map(|value| {
                    let diff = data_mean - *value as f64;

                    diff * diff
                })
                .sum::<f64>()
                / count as f64;

            Some(variance.sqrt())
        }
        _ => None,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_abbreviate_num() {
        assert_eq!(abbreviate_num(1_000), "1k");
        assert_eq!(abbreviate_num(1_000_000), "1.0M");
        assert_eq!(abbreviate_num(1_234_567), "1.2M");
    }
}
