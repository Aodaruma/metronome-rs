use crate::config::{BPM_MAX, BPM_MIN, CLICK_OFFSET_MAX_MS, CLICK_OFFSET_MIN_MS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatEvent {
    pub beat_index: u8,
    pub beats_per_bar: u8,
    pub is_accent: bool,
}

#[derive(Debug, Clone)]
pub struct BeatScheduler {
    threshold: u64,
    phase_increment: u64,
    phase: u64,
    beat_index: u8,
    running: bool,
    force_first_click: bool,
    pending_current_click: bool,
    early_next_click_fired: bool,
}

impl BeatScheduler {
    pub fn new(sample_rate_hz: u32) -> Self {
        Self {
            threshold: u64::from(sample_rate_hz) * 60_000,
            phase_increment: 120_000,
            phase: 0,
            beat_index: 0,
            running: false,
            force_first_click: false,
            pending_current_click: false,
            early_next_click_fired: false,
        }
    }

    pub fn set_tempo(&mut self, bpm_milli: u32, beat_unit: u8) {
        let bpm_milli = bpm_milli.clamp(BPM_MIN, BPM_MAX);
        let beat_unit = match beat_unit {
            2 | 4 | 8 | 16 => beat_unit,
            _ => 4,
        };
        self.phase_increment = u64::from(bpm_milli) * u64::from(beat_unit) / 4;
    }

    pub fn start(&mut self) {
        self.phase = 0;
        self.beat_index = 0;
        self.running = true;
        self.force_first_click = true;
        self.pending_current_click = false;
        self.early_next_click_fired = false;
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.force_first_click = false;
        self.pending_current_click = false;
        self.early_next_click_fired = false;
    }

    pub fn advance_frame(
        &mut self,
        bpm_milli: u32,
        beats_per_bar: u8,
        beat_unit: u8,
        click_offset_ms: i32,
    ) -> Option<BeatEvent> {
        self.set_tempo(bpm_milli, beat_unit);
        if !self.running {
            return None;
        }

        let beats_per_bar = beats_per_bar.clamp(1, 16);
        let click_offset_ms = click_offset_ms.clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS);
        let mut event = None;

        if self.force_first_click {
            self.force_first_click = false;
            if click_offset_ms <= 0 {
                event = Some(self.event_for(self.beat_index, beats_per_bar));
            } else {
                self.pending_current_click = true;
            }
        }

        if event.is_none() {
            if click_offset_ms >= 0 {
                let target_phase = self.offset_phase(click_offset_ms);
                if self.pending_current_click && self.phase >= target_phase {
                    self.pending_current_click = false;
                    event = Some(self.event_for(self.beat_index, beats_per_bar));
                }
            } else {
                let early_phase = self.threshold - self.offset_phase(click_offset_ms.abs()).max(1);
                if !self.early_next_click_fired && self.phase >= early_phase {
                    self.early_next_click_fired = true;
                    event = Some(self.event_for(self.next_beat(beats_per_bar), beats_per_bar));
                }
            }
        }

        self.phase += self.phase_increment;
        if self.phase >= self.threshold {
            self.phase -= self.threshold;
            self.beat_index = self.next_beat(beats_per_bar);
            if click_offset_ms >= 0 {
                self.pending_current_click = true;
            } else {
                self.early_next_click_fired = false;
            }
        }

        event
    }

    fn offset_phase(&self, offset_ms: i32) -> u64 {
        let frames = (u64::from(offset_ms.unsigned_abs()) * self.threshold) / 60_000_000;
        (frames * self.phase_increment).min(self.threshold.saturating_sub(1))
    }

    fn next_beat(&self, beats_per_bar: u8) -> u8 {
        (self.beat_index + 1) % beats_per_bar
    }

    fn event_for(&self, beat_index: u8, beats_per_bar: u8) -> BeatEvent {
        BeatEvent {
            beat_index,
            beats_per_bar,
            is_accent: beat_index == 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BeatScheduler;

    fn collect_intervals(bpm_milli: u32, beat_unit: u8, offset_ms: i32) -> Vec<u64> {
        let mut scheduler = BeatScheduler::new(48_000);
        scheduler.start();

        let mut hits = Vec::new();
        for frame in 0..(48_000 * 60) {
            if scheduler
                .advance_frame(bpm_milli, 4, beat_unit, offset_ms)
                .is_some()
            {
                hits.push(frame);
            }
            if hits.len() >= 16 {
                break;
            }
        }

        hits.windows(2).map(|pair| pair[1] - pair[0]).collect()
    }

    #[test]
    fn produces_stable_120_bpm_intervals() {
        let intervals = collect_intervals(120_000, 4, 0);
        assert!(!intervals.is_empty());
        assert!(intervals.iter().all(|&interval| interval == 24_000));
    }

    #[test]
    fn click_offset_does_not_change_tempo() {
        let delayed = collect_intervals(120_000, 4, 30);
        let early = collect_intervals(120_000, 4, -30);
        assert!(delayed.iter().all(|&interval| interval == 24_000));
        assert!(early.iter().skip(1).all(|&interval| interval == 24_000));
    }

    #[test]
    fn beat_unit_scales_the_click_interval() {
        assert!(
            collect_intervals(120_000, 2, 0)
                .iter()
                .all(|&interval| interval == 48_000)
        );
        assert!(
            collect_intervals(120_000, 8, 0)
                .iter()
                .all(|&interval| interval == 12_000)
        );
        assert!(
            collect_intervals(120_000, 16, 0)
                .iter()
                .all(|&interval| interval == 6_000)
        );
    }
}
