/// Convert page number and limit to offset for pagination
pub const fn page_to_offset(page: u32, limit: u32) -> u32 {
    (page - 1).saturating_mul(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_to_offset_first_page() {
        assert_eq!(page_to_offset(1, 50), 0);
    }

    #[test]
    fn page_to_offset_second_page() {
        assert_eq!(page_to_offset(2, 50), 50);
    }

    #[test]
    fn page_to_offset_custom_limit() {
        assert_eq!(page_to_offset(3, 10), 20);
    }

    #[test]
    fn page_to_offset_limit_one() {
        assert_eq!(page_to_offset(5, 1), 4);
    }
}
