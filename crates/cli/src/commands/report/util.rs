/// Abbreviates a number to a human-readable format.
pub fn abbreviate_num(num: u64) -> String {
    if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{}k", num / 1_000)
    } else {
        format!("{}", num)
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
