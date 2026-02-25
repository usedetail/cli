use rand::Rng;

use super::{NOISE, SPEED};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BandStyle {
    Idle,
    Active,
    Complete,
}

pub(super) struct SortState {
    pub(super) array: Vec<usize>,
    pub(super) source_array: Vec<usize>,
    pub(super) current_scan_index: Option<usize>,
    pub(super) base_window_size: usize,
    pub(super) scan_complete: bool,
    pub(super) complete_scan_index: Option<usize>,
    pub(super) current_window_size: usize,
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

    pub(super) fn reset_flags(&mut self) {
        self.current_scan_index = None;
        self.scan_complete = false;
        self.complete_scan_index = None;
        self.current_window_size = 0;
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

pub(super) fn generate_array(array_size: usize, rng: &mut impl Rng) -> Vec<usize> {
    let mut array: Vec<usize> = (1..=array_size).collect();
    if array_size <= 1 {
        return array;
    }

    let noise_percent = (f32::from(NOISE) / 100.0) * 0.5;
    let max_swap_dist = ((array_size as f32) * noise_percent).floor() as usize;
    let max_swap_dist = max_swap_dist.max(1);

    for i in (1..array.len()).rev() {
        let min_j = i.saturating_sub(max_swap_dist);
        let j = rng.random_range(min_j..=i);
        array.swap(i, j);
    }

    array
}

pub(super) fn place_target_value(array: &mut [usize], index: usize) {
    let target = index + 1;
    if array[index] == target {
        return;
    }

    if let Some(found_offset) = array[index + 1..].iter().position(|v| *v == target) {
        array.swap(index, index + 1 + found_offset);
    }
}
