use std::collections::VecDeque;

use crate::config::{BPM_MAX, BPM_MIN, CLICK_OFFSET_MAX_MS, CLICK_OFFSET_MIN_MS};

const MAX_DELAYED_EVENTS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatEvent {
    pub beat_index: u8,
    pub beats_per_bar: u8,
    pub subdivision_index: u8,
    pub subdivisions_per_beat: u8,
    pub is_accent: bool,
}

#[derive(Debug, Clone)]
pub struct BeatScheduler {
    threshold: u64,
    phase_increment: u64,
    phase: u64,
    beat_index: u8,
    subdivision_index: u8,
    running: bool,
    force_first_click: bool,
    pending_current_click: bool,
    early_next_click_fired: bool,
    frame_index: u64,
    delayed_events: VecDeque<(u64, BeatEvent)>,
    active_offset_ms: i32,
}

impl BeatScheduler {
    pub fn new(sample_rate_hz: u32) -> Self {
        Self {
            threshold: u64::from(sample_rate_hz) * 60_000,
            phase_increment: 120_000,
            phase: 0,
            beat_index: 0,
            subdivision_index: 0,
            running: false,
            force_first_click: false,
            pending_current_click: false,
            early_next_click_fired: false,
            frame_index: 0,
            delayed_events: VecDeque::with_capacity(MAX_DELAYED_EVENTS),
            active_offset_ms: 0,
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
        self.subdivision_index = 0;
        self.running = true;
        self.force_first_click = true;
        self.pending_current_click = false;
        self.early_next_click_fired = false;
        self.frame_index = 0;
        self.delayed_events.clear();
        self.active_offset_ms = 0;
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.force_first_click = false;
        self.pending_current_click = false;
        self.early_next_click_fired = false;
        self.delayed_events.clear();
        self.active_offset_ms = 0;
    }

    pub fn advance_frame(
        &mut self,
        bpm_milli: u32,
        beats_per_bar: u8,
        beat_unit: u8,
        subdivisions_per_beat: u8,
        click_offset_ms: i32,
    ) -> Option<BeatEvent> {
        self.set_tempo(bpm_milli, beat_unit);
        if !self.running {
            return None;
        }

        let beats_per_bar = beats_per_bar.clamp(1, 16);
        let subdivisions_per_beat = subdivisions_per_beat.clamp(1, 8);
        let subdivision_threshold = self.threshold / u64::from(subdivisions_per_beat);
        self.phase %= subdivision_threshold;
        self.subdivision_index %= subdivisions_per_beat;
        let click_offset_ms = click_offset_ms.clamp(CLICK_OFFSET_MIN_MS, CLICK_OFFSET_MAX_MS);
        if self.active_offset_ms != click_offset_ms {
            self.delayed_events.clear();
            self.pending_current_click = false;
            self.early_next_click_fired = false;
            self.active_offset_ms = click_offset_ms;
        }

        let event = if click_offset_ms > 0 {
            if let Some(event) = self.advance_offset_core(
                beats_per_bar,
                subdivisions_per_beat,
                subdivision_threshold,
                0,
            ) {
                let due_frame = self
                    .frame_index
                    .saturating_add(self.offset_frames(click_offset_ms));
                if self.delayed_events.len() == MAX_DELAYED_EVENTS {
                    self.delayed_events.pop_front();
                }
                self.delayed_events.push_back((due_frame, event));
            }
            if self
                .delayed_events
                .front()
                .is_some_and(|(due_frame, _)| *due_frame <= self.frame_index)
            {
                self.delayed_events.pop_front().map(|(_, event)| event)
            } else {
                None
            }
        } else {
            self.advance_offset_core(
                beats_per_bar,
                subdivisions_per_beat,
                subdivision_threshold,
                click_offset_ms,
            )
        };
        self.frame_index = self.frame_index.saturating_add(1);
        event
    }

    fn advance_offset_core(
        &mut self,
        beats_per_bar: u8,
        subdivisions_per_beat: u8,
        subdivision_threshold: u64,
        click_offset_ms: i32,
    ) -> Option<BeatEvent> {
        let mut event = None;

        if self.force_first_click {
            self.force_first_click = false;
            if click_offset_ms <= 0 {
                event = Some(self.event_for(
                    self.beat_index,
                    self.subdivision_index,
                    beats_per_bar,
                    subdivisions_per_beat,
                ));
            } else {
                self.pending_current_click = true;
            }
        }

        if event.is_none() {
            if click_offset_ms >= 0 {
                let target_phase = self
                    .offset_phase(click_offset_ms)
                    .min(subdivision_threshold.saturating_sub(1));
                // The requested offset can be as long as (or longer than) the
                // current click interval. In that case `target_phase` is
                // clamped just before the boundary and may not be exactly
                // reachable by the phase increment. Fire on the frame that
                // crosses the target instead of waiting forever.
                let next_phase = self.phase.saturating_add(self.phase_increment);
                if self.pending_current_click
                    && (self.phase >= target_phase || next_phase >= target_phase)
                {
                    self.pending_current_click = false;
                    event = Some(self.event_for(
                        self.beat_index,
                        self.subdivision_index,
                        beats_per_bar,
                        subdivisions_per_beat,
                    ));
                }
            } else {
                let offset_phase = self.offset_phase(click_offset_ms.abs());
                let whole_intervals = offset_phase / subdivision_threshold;
                let remaining_phase = offset_phase % subdivision_threshold;
                let early_phase = if remaining_phase == 0 {
                    subdivision_threshold.saturating_sub(1)
                } else {
                    subdivision_threshold - remaining_phase
                };
                let next_phase = self.phase.saturating_add(self.phase_increment);
                if !self.early_next_click_fired
                    && (self.phase >= early_phase || next_phase >= early_phase)
                {
                    self.early_next_click_fired = true;
                    let (beat_index, subdivision_index) = self.future_position(
                        beats_per_bar,
                        subdivisions_per_beat,
                        whole_intervals.saturating_add(1),
                    );
                    event = Some(self.event_for(
                        beat_index,
                        subdivision_index,
                        beats_per_bar,
                        subdivisions_per_beat,
                    ));
                }
            }
        }

        self.phase += self.phase_increment;
        if self.phase >= subdivision_threshold {
            self.phase -= subdivision_threshold;
            (self.beat_index, self.subdivision_index) =
                self.next_position(beats_per_bar, subdivisions_per_beat);
            if click_offset_ms >= 0 {
                self.pending_current_click = true;
            } else {
                self.early_next_click_fired = false;
            }
        }

        event
    }

    fn offset_phase(&self, offset_ms: i32) -> u64 {
        self.offset_frames(offset_ms)
            .saturating_mul(self.phase_increment)
    }

    fn offset_frames(&self, offset_ms: i32) -> u64 {
        (u64::from(offset_ms.unsigned_abs()) * self.threshold) / 60_000_000
    }

    fn next_position(&self, beats_per_bar: u8, subdivisions_per_beat: u8) -> (u8, u8) {
        let next_subdivision = self.subdivision_index + 1;
        if next_subdivision >= subdivisions_per_beat {
            ((self.beat_index + 1) % beats_per_bar, 0)
        } else {
            (self.beat_index, next_subdivision)
        }
    }

    fn future_position(
        &self,
        beats_per_bar: u8,
        subdivisions_per_beat: u8,
        steps: u64,
    ) -> (u8, u8) {
        let subdivisions_per_bar = u64::from(beats_per_bar) * u64::from(subdivisions_per_beat);
        let current = u64::from(self.beat_index) * u64::from(subdivisions_per_beat)
            + u64::from(self.subdivision_index);
        let future = (current + steps) % subdivisions_per_bar;
        (
            (future / u64::from(subdivisions_per_beat)) as u8,
            (future % u64::from(subdivisions_per_beat)) as u8,
        )
    }

    fn event_for(
        &self,
        beat_index: u8,
        subdivision_index: u8,
        beats_per_bar: u8,
        subdivisions_per_beat: u8,
    ) -> BeatEvent {
        BeatEvent {
            beat_index,
            beats_per_bar,
            subdivision_index,
            subdivisions_per_beat,
            is_accent: beat_index == 0 && subdivision_index == 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BeatEvent, BeatScheduler};

    fn collect_events(
        bpm_milli: u32,
        beat_unit: u8,
        subdivisions_per_beat: u8,
        offset_ms: i32,
        count: usize,
    ) -> Vec<(u64, BeatEvent)> {
        let mut scheduler = BeatScheduler::new(48_000);
        scheduler.start();

        let mut events = Vec::new();
        for frame in 0..(48_000 * 60) {
            if let Some(event) =
                scheduler.advance_frame(bpm_milli, 4, beat_unit, subdivisions_per_beat, offset_ms)
            {
                events.push((frame, event));
            }
            if events.len() >= count {
                break;
            }
        }
        events
    }

    fn collect_intervals(
        bpm_milli: u32,
        beat_unit: u8,
        subdivisions_per_beat: u8,
        offset_ms: i32,
    ) -> Vec<u64> {
        collect_events(bpm_milli, beat_unit, subdivisions_per_beat, offset_ms, 16)
            .windows(2)
            .map(|pair| pair[1].0 - pair[0].0)
            .collect()
    }

    #[test]
    fn produces_stable_120_bpm_intervals() {
        let intervals = collect_intervals(120_000, 4, 1, 0);
        assert!(!intervals.is_empty());
        assert!(intervals.iter().all(|&interval| interval == 24_000));
    }

    #[test]
    fn click_offset_does_not_change_tempo() {
        let delayed = collect_intervals(120_000, 4, 1, 30);
        let early = collect_intervals(120_000, 4, 1, -30);
        assert!(delayed.iter().all(|&interval| interval == 24_000));
        assert!(early.iter().skip(1).all(|&interval| interval == 24_000));
    }

    #[test]
    fn beat_unit_scales_the_click_interval() {
        assert!(
            collect_intervals(120_000, 2, 1, 0)
                .iter()
                .all(|&interval| interval == 48_000)
        );
        assert!(
            collect_intervals(120_000, 8, 1, 0)
                .iter()
                .all(|&interval| interval == 12_000)
        );
        assert!(
            collect_intervals(120_000, 16, 1, 0)
                .iter()
                .all(|&interval| interval == 6_000)
        );
    }

    #[test]
    fn subdivisions_two_through_eight_produce_even_clicks() {
        for subdivisions in 2..=8 {
            let expected = 24_000.0 / f64::from(subdivisions);
            let intervals = collect_intervals(120_000, 4, subdivisions, 0);
            assert!(!intervals.is_empty());
            assert!(
                intervals
                    .iter()
                    .all(|&interval| (interval as f64 - expected).abs() <= 1.0),
                "subdivision {subdivisions} intervals: {intervals:?}"
            );
        }
    }

    #[test]
    fn click_offset_keeps_subdivision_tempo_stable() {
        let delayed = collect_intervals(120_000, 4, 8, 30);
        let early = collect_intervals(120_000, 4, 8, -30);
        assert!(delayed.iter().all(|&interval| interval == 3_000));
        assert!(early.iter().skip(1).all(|&interval| interval == 3_000));
    }

    #[test]
    fn long_positive_offset_does_not_silence_high_bpm() {
        for bpm_milli in [300_000, 301_000, 1_000_000] {
            let intervals = collect_intervals(bpm_milli, 4, 1, 200);
            assert!(
                !intervals.is_empty(),
                "BPM {} stopped producing clicks",
                bpm_milli / 1_000
            );
        }
    }

    #[test]
    fn long_positive_offset_does_not_silence_subdivisions() {
        for subdivisions in 2..=8 {
            let intervals = collect_intervals(120_000, 4, subdivisions, 200);
            assert!(
                !intervals.is_empty(),
                "subdivision {subdivisions} stopped producing clicks"
            );
        }
    }

    #[test]
    fn positive_offset_preserves_the_full_delay_across_subdivision_intervals() {
        let on_grid = collect_events(120_000, 4, 8, 0, 12);
        let delayed = collect_events(120_000, 4, 8, 100, 12);
        assert_eq!(on_grid.len(), delayed.len());
        for ((grid_frame, grid_event), (delayed_frame, delayed_event)) in
            on_grid.iter().zip(&delayed)
        {
            assert_eq!(delayed_frame - grid_frame, 4_800);
            assert_eq!(delayed_event, grid_event);
        }
    }

    #[test]
    fn negative_offset_advances_subdivision_identity_across_intervals() {
        let early = collect_events(120_000, 4, 8, -100, 4);
        assert_eq!(early[0].0, 0);
        assert_eq!(early[0].1.subdivision_index, 0);
        assert!(early[1].0.abs_diff(1_200) <= 1);
        assert_eq!(early[1].1.subdivision_index, 2);
        assert!(early[2].0.abs_diff(4_200) <= 1);
        assert_eq!(early[2].1.subdivision_index, 3);
    }

    #[test]
    fn subdivision_events_preserve_beat_and_accent_identity() {
        let mut scheduler = BeatScheduler::new(48_000);
        scheduler.start();
        let mut events = Vec::new();
        for _ in 0..48_000 {
            if let Some(event) = scheduler.advance_frame(120_000, 4, 4, 4, 0) {
                events.push(event);
            }
            if events.len() == 5 {
                break;
            }
        }

        assert_eq!(events[0].beat_index, 0);
        assert_eq!(events[0].subdivision_index, 0);
        assert!(events[0].is_accent);
        for (index, event) in events[1..4].iter().enumerate() {
            assert_eq!(event.beat_index, 0);
            assert_eq!(event.subdivision_index, index as u8 + 1);
            assert!(!event.is_accent);
        }
        assert_eq!(events[4].beat_index, 1);
        assert_eq!(events[4].subdivision_index, 0);
        assert!(!events[4].is_accent);
    }
}
