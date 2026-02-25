use rand::Rng;

use super::{NOISE, SPEED};

const NOISE_SPREAD_FACTOR: f32 = 0.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BandStyle {
    Idle,
    Active,
    Complete,
}

pub(super) struct SortState {
    array: Vec<usize>,
    source_array: Vec<usize>,
    current_scan_index: Option<usize>,
    base_window_size: usize,
    scan_complete: bool,
    complete_scan_index: Option<usize>,
    current_window_size: usize,
}

impl SortState {
    pub(super) fn new(array_size: usize, rng: &mut impl Rng) -> Self {
        let array = generate_array(array_size, rng);
        let mut state = Self {
            source_array: array.clone(),
            array,
            current_scan_index: None,
            base_window_size: base_window_size(array_size),
            scan_complete: false,
            complete_scan_index: None,
            current_window_size: 0,
        };
        state.reset_flags();
        state
    }

    pub(super) fn reset(&mut self, rng: &mut impl Rng) {
        self.array = generate_array(self.array.len(), rng);
        self.source_array = self.array.clone();
        self.base_window_size = base_window_size(self.array.len());
        self.reset_flags();
    }

    pub(super) fn len(&self) -> usize {
        self.array.len()
    }

    pub(super) fn source_array(&self) -> &[usize] {
        &self.source_array
    }

    pub(super) fn current_scan_index(&self) -> Option<usize> {
        self.current_scan_index
    }

    pub(super) fn scan_complete(&self) -> bool {
        self.scan_complete
    }

    pub(super) fn apply_sort_step(&mut self, index: usize) {
        self.current_window_size = self.base_window_size.max(1);
        self.current_scan_index = Some(index);
        place_target_value(&mut self.array, index);
    }

    pub(super) fn finalize_sort_pass(&mut self) {
        self.scan_complete = true;
        self.current_scan_index = None;
        self.current_window_size = 0;
        self.complete_scan_index = None;
    }

    pub(super) fn set_completion_index(&mut self, index: usize) {
        self.complete_scan_index = Some(index);
    }

    pub(super) fn style_for_index(&self, index: usize) -> BandStyle {
        if self.scan_complete {
            if self.complete_scan_index.is_some_and(|done| index <= done) {
                return BandStyle::Complete;
            }
            return BandStyle::Idle;
        }

        if let Some(scan_start) = self.current_scan_index {
            if in_window(index, scan_start, self.current_window_size) {
                return BandStyle::Active;
            }
        }

        BandStyle::Idle
    }

    fn reset_flags(&mut self) {
        self.current_scan_index = None;
        self.scan_complete = false;
        self.complete_scan_index = None;
        self.current_window_size = 0;
    }
}

fn in_window(index: usize, start: usize, len: usize) -> bool {
    index >= start && index < start.saturating_add(len)
}

fn base_window_size(array_size: usize) -> usize {
    let base = (array_size / 100).clamp(10, 100);
    base.min(array_size.max(1))
}

pub(super) fn sort_delay_ms() -> u64 {
    u64::from(100u8.saturating_sub(SPEED)) / 2
}

fn generate_array(array_size: usize, rng: &mut impl Rng) -> Vec<usize> {
    let mut array: Vec<usize> = (1..=array_size).collect();
    if array_size <= 1 {
        return array;
    }

    let noise_percent = (f32::from(NOISE) / 100.0) * NOISE_SPREAD_FACTOR;
    let max_swap_dist = ((array_size as f32) * noise_percent).floor() as usize;
    let max_swap_dist = max_swap_dist.clamp(1, array_size.saturating_sub(1).max(1));

    for i in (1..array.len()).rev() {
        let min_j = i.saturating_sub(max_swap_dist);
        let j = rng.random_range(min_j..=i);
        array.swap(i, j);
    }

    array
}

fn place_target_value(array: &mut [usize], index: usize) {
    let target = index + 1;
    if array[index] == target {
        return;
    }

    if let Some(found_offset) = array[index + 1..].iter().position(|v| *v == target) {
        array.swap(index, index + 1 + found_offset);
    }
}

#[cfg(test)]
mod tests {
    use rand::{rngs::SmallRng, SeedableRng};

    use super::*;

    #[test]
    fn generated_array_is_permutation() {
        let mut rng = SmallRng::seed_from_u64(7);
        let mut array = generate_array(128, &mut rng);
        array.sort_unstable();
        let expected: Vec<usize> = (1..=128).collect();
        assert_eq!(array, expected);
    }

    #[test]
    fn place_target_value_moves_target_into_place() {
        let mut array = vec![3, 2, 1, 4];
        place_target_value(&mut array, 0);
        assert_eq!(array, vec![1, 2, 3, 4]);
    }

    #[test]
    fn style_for_index_respects_active_window() {
        let mut rng = SmallRng::seed_from_u64(1);
        let mut state = SortState::new(64, &mut rng);
        state.scan_complete = false;
        state.current_scan_index = Some(10);
        state.current_window_size = 8;

        assert_eq!(state.style_for_index(11), BandStyle::Active);
        assert_eq!(state.style_for_index(12), BandStyle::Active);
        assert_eq!(state.style_for_index(14), BandStyle::Active);
        assert_eq!(state.style_for_index(15), BandStyle::Active);
    }

    #[test]
    fn style_for_index_marks_completion() {
        let mut rng = SmallRng::seed_from_u64(9);
        let mut state = SortState::new(32, &mut rng);
        state.scan_complete = true;
        state.complete_scan_index = Some(5);

        assert_eq!(state.style_for_index(3), BandStyle::Complete);
        assert_eq!(state.style_for_index(7), BandStyle::Idle);
    }
}
