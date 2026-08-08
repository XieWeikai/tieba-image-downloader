pub fn increase(current: usize, maximum: usize) -> usize {
    (current + (current / 4).max(1)).min(maximum.max(1))
}

pub fn decrease(current: usize) -> usize {
    (current / 2).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increases_gradually_without_exceeding_maximum() {
        assert_eq!(increase(1, 32), 2);
        assert_eq!(increase(16, 32), 20);
        assert_eq!(increase(31, 32), 32);
        assert_eq!(increase(32, 32), 32);
    }

    #[test]
    fn decreases_fast_but_never_reaches_zero() {
        assert_eq!(decrease(32), 16);
        assert_eq!(decrease(3), 1);
        assert_eq!(decrease(1), 1);
    }
}
