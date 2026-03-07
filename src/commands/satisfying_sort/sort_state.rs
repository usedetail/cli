use rand::{Rng, RngExt};

use super::numeric::{floor_f32_to_usize, usize_to_f32};
use super::{NOISE, SPEED};

const NOISE_SPREAD_FACTOR: f32 = 0.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BandStyle {
    Idle,
    Active,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Sorting { scan_start: usize },
    Completion { done: Option<usize> },
}

pub(super) struct SortState {
    source_array: Vec<usize>,
    phase: Phase,
}

impl SortState {
    pub(super) fn new(array_size: usize, rng: &mut impl Rng) -> Self {
        let source_array = generate_array(array_size, rng);
        Self {
            source_array,
            phase: Phase::Idle,
        }
    }

    pub(super) fn reset(&mut self, rng: &mut impl Rng) {
        self.source_array = generate_array(self.source_array.len(), rng);
        self.phase = Phase::Idle;
    }

    pub(super) const fn len(&self) -> usize {
        self.source_array.len()
    }

    pub(super) fn source_array(&self) -> &[usize] {
        &self.source_array
    }

    pub(super) const fn current_scan_index(&self) -> Option<usize> {
        match self.phase {
            Phase::Sorting { scan_start } => Some(scan_start),
            Phase::Idle | Phase::Completion { .. } => None,
        }
    }

    pub(super) const fn scan_complete(&self) -> bool {
        matches!(self.phase, Phase::Completion { .. })
    }

    pub(super) const fn apply_sort_step(&mut self, index: usize) {
        self.phase = Phase::Sorting { scan_start: index };
    }

    pub(super) const fn finalize_sort_pass(&mut self) {
        self.phase = Phase::Completion { done: None };
    }

    pub(super) const fn set_completion_index(&mut self, index: usize) {
        self.phase = Phase::Completion { done: Some(index) };
    }

    pub(super) fn style_for_index_with_min_window(
        &self,
        index: usize,
        min_window_size: usize,
    ) -> BandStyle {
        match self.phase {
            Phase::Completion { done } => {
                if done.is_some_and(|done| index <= done) {
                    return BandStyle::Complete;
                }
            }
            Phase::Sorting { scan_start } => {
                let window_size =
                    base_window_size(self.source_array.len()).max(min_window_size.max(1));
                if in_window(index, scan_start, window_size) {
                    return BandStyle::Active;
                }
            }
            Phase::Idle => {}
        }

        BandStyle::Idle
    }
}

const fn in_window(index: usize, start: usize, len: usize) -> bool {
    index >= start && index < start.saturating_add(len)
}

fn base_window_size(array_size: usize) -> usize {
    let base = (array_size / 100).clamp(10, 100);
    base.min(array_size.max(1))
}

pub(super) fn sort_delay_ms() -> u64 {
    u64::from(100_u8.saturating_sub(SPEED)) / 2
}

fn generate_array(array_size: usize, rng: &mut impl Rng) -> Vec<usize> {
    let mut array: Vec<usize> = (1..=array_size).collect();
    if array_size <= 1 {
        return array;
    }

    let noise_percent = (f32::from(NOISE) / 100.0) * NOISE_SPREAD_FACTOR;
    let max_swap_dist = floor_f32_to_usize(usize_to_f32(array_size) * noise_percent);
    let max_swap_dist = max_swap_dist.clamp(1, array_size.saturating_sub(1));

    for i in (1..array.len()).rev() {
        let min_j = i.saturating_sub(max_swap_dist);
        let j = rng.random_range(min_j..=i);
        array.swap(i, j);
    }

    array
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
    fn state_source_array_is_permutation_after_reset() {
        let mut rng = SmallRng::seed_from_u64(13);
        let mut state = SortState::new(128, &mut rng);
        state.reset(&mut rng);

        let mut array = state.source_array().to_vec();
        array.sort_unstable();
        let expected: Vec<usize> = (1..=128).collect();
        assert_eq!(array, expected);
    }

    #[test]
    fn style_for_index_respects_active_window() {
        let mut rng = SmallRng::seed_from_u64(1);
        let mut state = SortState::new(64, &mut rng);
        state.phase = Phase::Sorting { scan_start: 10 };

        assert_eq!(
            state.style_for_index_with_min_window(11, 0),
            BandStyle::Active
        );
        assert_eq!(
            state.style_for_index_with_min_window(12, 0),
            BandStyle::Active
        );
        assert_eq!(
            state.style_for_index_with_min_window(14, 0),
            BandStyle::Active
        );
        assert_eq!(
            state.style_for_index_with_min_window(15, 0),
            BandStyle::Active
        );
    }

    #[test]
    fn style_for_index_marks_completion() {
        let mut rng = SmallRng::seed_from_u64(9);
        let mut state = SortState::new(32, &mut rng);
        state.phase = Phase::Completion { done: Some(5) };

        assert_eq!(
            state.style_for_index_with_min_window(3, 0),
            BandStyle::Complete
        );
        assert_eq!(state.style_for_index_with_min_window(7, 0), BandStyle::Idle);
    }

    #[test]
    fn phase_helpers_match_state_transitions() {
        let mut rng = SmallRng::seed_from_u64(21);
        let mut state = SortState::new(32, &mut rng);
        assert_eq!(state.current_scan_index(), None);
        assert!(!state.scan_complete());

        state.apply_sort_step(4);
        assert_eq!(state.current_scan_index(), Some(4));
        assert!(!state.scan_complete());

        state.finalize_sort_pass();
        assert_eq!(state.current_scan_index(), None);
        assert!(state.scan_complete());

        state.reset(&mut rng);
        assert_eq!(state.current_scan_index(), None);
        assert!(!state.scan_complete());
    }
}
