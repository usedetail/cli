use num_traits::ToPrimitive;

pub(super) fn usize_to_f32(value: usize) -> f32 {
    value.to_f32().unwrap_or(f32::MAX)
}

pub(super) fn floor_f32_to_usize(value: f32) -> usize {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    if value.is_infinite() {
        return usize::MAX;
    }

    value.floor().to_usize().unwrap_or(usize::MAX)
}

pub(super) fn round_f32_to_usize(value: f32) -> usize {
    if value.is_nan() || value <= 0.0 {
        return 0;
    }
    if value.is_infinite() {
        return usize::MAX;
    }

    value.round().to_usize().unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_maps_nan_and_negative_to_zero() {
        assert_eq!(floor_f32_to_usize(f32::NAN), 0);
        assert_eq!(floor_f32_to_usize(-1.2), 0);
        assert_eq!(floor_f32_to_usize(3.9), 3);
    }

    #[test]
    fn round_maps_nan_and_negative_to_zero() {
        assert_eq!(round_f32_to_usize(f32::NAN), 0);
        assert_eq!(round_f32_to_usize(-0.1), 0);
        assert_eq!(round_f32_to_usize(3.5), 4);
    }
}
