use crate::config::{BEAT_UNIT_MAX, BEAT_UNIT_MIN, BPM_MAX, BPM_MIN};

// This is divisible by every supported subdivision count (1..=8) and by the
// swing interpolation denominator (100). Keeping bar positions in this fixed
// point scale lets odd subdivisions be warped without floating-point drift.
const POSITION_SCALE: u64 = 252_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwingGrid {
    Quarter,
    Eighth,
    Sixteenth,
}

impl SwingGrid {
    const fn denominator(self) -> u64 {
        match self {
            Self::Quarter => 4,
            Self::Eighth => 8,
            Self::Sixteenth => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwingSettings {
    pub grid: SwingGrid,
    pub amount_percent: i16,
}

impl SwingSettings {
    pub const fn new(grid: SwingGrid, amount_percent: i16) -> Self {
        Self {
            grid,
            amount_percent,
        }
    }

    fn normalized(self) -> Self {
        Self {
            grid: self.grid,
            amount_percent: self.amount_percent.clamp(-100, 100),
        }
    }
}

impl Default for SwingSettings {
    fn default() -> Self {
        Self::new(SwingGrid::Eighth, 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatEvent {
    pub beat_index: u8,
    pub beats_per_bar: u8,
    pub subdivision_index: u8,
    pub subdivisions_per_beat: u8,
    pub is_accent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pattern {
    beats_per_bar: u8,
    beat_unit: u8,
    subdivisions_per_beat: u8,
    swing: SwingSettings,
}

impl Pattern {
    fn new(
        beats_per_bar: u8,
        beat_unit: u8,
        subdivisions_per_beat: u8,
        swing: SwingSettings,
    ) -> Self {
        Self {
            beats_per_bar: beats_per_bar.clamp(1, 16),
            beat_unit: beat_unit.clamp(BEAT_UNIT_MIN, BEAT_UNIT_MAX),
            subdivisions_per_beat: subdivisions_per_beat.clamp(1, 8),
            swing: swing.normalized(),
        }
    }

    fn events_per_bar(self) -> u16 {
        u16::from(self.beats_per_bar) * u16::from(self.subdivisions_per_beat)
    }
}

#[derive(Debug, Clone)]
pub struct BeatScheduler {
    // A notated beat occupies `beat_threshold_scaled` phase units. Phase is
    // advanced once per audio sample, so BeatEvents are emitted directly on
    // their rhythmic sample frame and can be offset later by the audio layer.
    beat_threshold_scaled: u64,
    bar_phase_scaled: u64,
    next_event_ordinal: u16,
    running: bool,
    active_pattern: Option<Pattern>,
}

impl BeatScheduler {
    pub fn new(sample_rate_hz: u32) -> Self {
        Self {
            beat_threshold_scaled: u64::from(sample_rate_hz)
                .saturating_mul(60_000)
                .saturating_mul(4)
                .saturating_mul(POSITION_SCALE),
            bar_phase_scaled: 0,
            next_event_ordinal: 0,
            running: false,
            active_pattern: None,
        }
    }

    pub fn start(&mut self) {
        self.bar_phase_scaled = 0;
        self.next_event_ordinal = 0;
        self.running = true;
        self.active_pattern = None;
    }

    pub fn stop(&mut self) {
        self.running = false;
        self.bar_phase_scaled = 0;
        self.next_event_ordinal = 0;
        self.active_pattern = None;
    }

    /// Advances the rhythmic clock by one audio sample frame.
    ///
    /// Every emitted event is already placed on the swung rhythmic grid. At
    /// ±100%, multiple events can intentionally share one sample frame, so a
    /// callback is used instead of returning a single event. Sound-source
    /// offsets should be applied after this scheduler so they never alter the
    /// BeatEvent used for visual feedback.
    pub fn advance_frame(
        &mut self,
        bpm_milli: u32,
        beats_per_bar: u8,
        beat_unit: u8,
        subdivisions_per_beat: u8,
        swing: SwingSettings,
        mut emit: impl FnMut(BeatEvent),
    ) {
        if !self.running {
            return;
        }

        // Complete a pending bar with its previous pattern first. A +100%
        // offbeat can land exactly on the next bar line and must be emitted on
        // the same sample frame as the new downbeat instead of being dropped.
        if let Some(previous) = self.active_pattern {
            self.wrap_completed_bars(previous, &mut emit);
        }

        let pattern = Pattern::new(beats_per_bar, beat_unit, subdivisions_per_beat, swing);
        self.apply_pattern_change(pattern);
        self.wrap_completed_bars(pattern, &mut emit);
        self.emit_due_events(pattern, self.bar_phase_scaled, &mut emit);

        let bpm_milli = bpm_milli.clamp(BPM_MIN, BPM_MAX);
        let phase_increment = u64::from(bpm_milli)
            .saturating_mul(u64::from(pattern.beat_unit))
            .saturating_mul(POSITION_SCALE);
        self.bar_phase_scaled = self.bar_phase_scaled.saturating_add(phase_increment);
    }

    fn wrap_completed_bars(&mut self, pattern: Pattern, emit: &mut impl FnMut(BeatEvent)) {
        let bar_length = self.bar_length_scaled(pattern);
        while self.bar_phase_scaled >= bar_length {
            self.emit_due_events(pattern, bar_length, emit);
            self.bar_phase_scaled -= bar_length;
            self.next_event_ordinal = 0;
        }
    }

    fn emit_due_events(
        &mut self,
        pattern: Pattern,
        through_phase: u64,
        emit: &mut impl FnMut(BeatEvent),
    ) {
        while self.next_event_ordinal < pattern.events_per_bar()
            && self.event_phase_scaled(pattern, self.next_event_ordinal) <= through_phase
        {
            let event = self.event_for(pattern, self.next_event_ordinal);
            self.next_event_ordinal += 1;
            emit(event);
        }
    }

    fn apply_pattern_change(&mut self, pattern: Pattern) {
        let Some(previous) = self.active_pattern.replace(pattern) else {
            return;
        };
        if previous == pattern {
            return;
        }

        // Swing changes retain event identity. If a slider moves an upcoming
        // event behind the playhead, it is emitted once on the next frame.
        if previous.beats_per_bar == pattern.beats_per_bar
            && previous.beat_unit == pattern.beat_unit
            && previous.subdivisions_per_beat == pattern.subdivisions_per_beat
        {
            return;
        }

        // Structural meter changes preserve the relative playhead position,
        // then select the first event at or ahead of the current sample.
        let previous_bar_length = self.bar_length_scaled(previous);
        let new_bar_length = self.bar_length_scaled(pattern);
        self.bar_phase_scaled = ((u128::from(self.bar_phase_scaled) * u128::from(new_bar_length))
            / u128::from(previous_bar_length)) as u64;
        self.next_event_ordinal = (0..pattern.events_per_bar())
            .find(|&ordinal| self.event_phase_scaled(pattern, ordinal) >= self.bar_phase_scaled)
            .unwrap_or(pattern.events_per_bar());
    }

    fn bar_length_scaled(&self, pattern: Pattern) -> u64 {
        self.beat_threshold_scaled
            .saturating_mul(u64::from(pattern.beats_per_bar))
    }

    fn event_phase_scaled(&self, pattern: Pattern, ordinal: u16) -> u64 {
        let straight = self
            .beat_threshold_scaled
            .saturating_mul(u64::from(ordinal))
            / u64::from(pattern.subdivisions_per_beat);
        let amount = i64::from(pattern.swing.amount_percent);
        if amount == 0 {
            return straight;
        }

        // Express the selected note value in notated-beat phase units. A
        // complete swing pair spans two selected grid notes.
        let grid = self
            .beat_threshold_scaled
            .saturating_mul(u64::from(pattern.beat_unit))
            / pattern.swing.grid.denominator();
        let pair_length = grid.saturating_mul(2);
        let pair_start = (straight / pair_length) * pair_length;

        // A partial pair at the end of a bar stays straight. Pairing restarts
        // at every bar, which fixes the downbeat and prevents cumulative drift.
        if pair_start.saturating_add(pair_length) > self.bar_length_scaled(pattern) {
            return straight;
        }

        let local = straight - pair_start;
        let first_weight = u64::try_from(100 + amount).expect("swing amount is normalized");
        let second_weight = u64::try_from(100 - amount).expect("swing amount is normalized");
        let warped_local = if local <= grid {
            ((u128::from(local) * u128::from(first_weight)) / 100) as u64
        } else {
            ((u128::from(grid) * u128::from(first_weight)
                + u128::from(local - grid) * u128::from(second_weight))
                / 100) as u64
        };
        pair_start + warped_local
    }

    fn event_for(&self, pattern: Pattern, ordinal: u16) -> BeatEvent {
        let subdivisions = u16::from(pattern.subdivisions_per_beat);
        let beat_index = (ordinal / subdivisions) as u8;
        let subdivision_index = (ordinal % subdivisions) as u8;
        BeatEvent {
            beat_index,
            beats_per_bar: pattern.beats_per_bar,
            subdivision_index,
            subdivisions_per_beat: pattern.subdivisions_per_beat,
            is_accent: beat_index == 0 && subdivision_index == 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BeatEvent, BeatScheduler, SwingGrid, SwingSettings};

    const SAMPLE_RATE: u32 = 48_000;

    fn collect_events(
        bpm_milli: u32,
        beats_per_bar: u8,
        beat_unit: u8,
        subdivisions_per_beat: u8,
        swing: SwingSettings,
        count: usize,
    ) -> Vec<(u64, BeatEvent)> {
        let mut scheduler = BeatScheduler::new(SAMPLE_RATE);
        scheduler.start();

        let mut events = Vec::new();
        for frame in 0..u64::from(SAMPLE_RATE) * 600 {
            scheduler.advance_frame(
                bpm_milli,
                beats_per_bar,
                beat_unit,
                subdivisions_per_beat,
                swing,
                |event| {
                    if events.len() < count {
                        events.push((frame, event));
                    }
                },
            );
            if events.len() >= count {
                break;
            }
        }
        events
    }

    fn frames(
        beats_per_bar: u8,
        beat_unit: u8,
        subdivisions_per_beat: u8,
        grid: SwingGrid,
        amount_percent: i16,
        count: usize,
    ) -> Vec<u64> {
        collect_events(
            120_000,
            beats_per_bar,
            beat_unit,
            subdivisions_per_beat,
            SwingSettings::new(grid, amount_percent),
            count,
        )
        .into_iter()
        .map(|(frame, _)| frame)
        .collect()
    }

    #[test]
    fn zero_swing_is_identical_for_every_grid() {
        let expected = frames(4, 4, 4, SwingGrid::Eighth, 0, 20);
        assert_eq!(expected, frames(4, 4, 4, SwingGrid::Quarter, 0, 20));
        assert_eq!(expected, frames(4, 4, 4, SwingGrid::Sixteenth, 0, 20));
        assert!(expected.windows(2).all(|pair| pair[1] - pair[0] == 6_000));
    }

    #[test]
    fn swing_extremes_overlap_the_previous_or_next_grid_point() {
        let late = frames(4, 4, 2, SwingGrid::Eighth, 100, 5);
        assert_eq!(late, vec![0, 24_000, 24_000, 48_000, 48_000]);

        let early = frames(4, 4, 2, SwingGrid::Eighth, -100, 5);
        assert_eq!(early, vec![0, 0, 24_000, 24_000, 48_000]);
    }

    #[test]
    fn positive_extreme_preserves_the_bar_end_offbeat() {
        let events = collect_events(
            120_000,
            4,
            4,
            2,
            SwingSettings::new(SwingGrid::Eighth, 100),
            9,
        );
        assert_eq!(events[7].0, 96_000);
        assert_eq!(events[8].0, 96_000);
        assert!(!events[7].1.is_accent);
        assert!(events[8].1.is_accent);
    }

    #[test]
    fn quarter_eighth_and_sixteenth_grids_warp_the_selected_offbeat() {
        let quarter = frames(4, 4, 4, SwingGrid::Quarter, 100, 9);
        assert_eq!(quarter[4], 48_000);
        assert_eq!(quarter[8], 48_000);

        let eighth = frames(4, 4, 4, SwingGrid::Eighth, 100, 9);
        assert_eq!(eighth[2], 24_000);
        assert_eq!(eighth[4], 24_000);

        let sixteenth = frames(4, 4, 4, SwingGrid::Sixteenth, 100, 9);
        assert_eq!(sixteenth[1], 12_000);
        assert_eq!(sixteenth[2], 12_000);
    }

    #[test]
    fn bar_downbeats_stay_fixed_across_common_meters() {
        for (beats_per_bar, beat_unit) in [(3, 3), (3, 4), (4, 4), (5, 5), (7, 7), (6, 8), (5, 16)]
        {
            let subdivisions = 5;
            let events_per_bar = usize::from(beats_per_bar) * usize::from(subdivisions);
            let events = collect_events(
                120_000,
                beats_per_bar,
                beat_unit,
                subdivisions,
                SwingSettings::new(SwingGrid::Eighth, 100),
                events_per_bar + 1,
            );
            let expected_bar_frames = 24_000 * u64::from(beats_per_bar) * 4 / u64::from(beat_unit);
            assert_eq!(events[0].0, 0, "{beats_per_bar}/{beat_unit}");
            assert_eq!(
                events[events_per_bar].0, expected_bar_frames,
                "{beats_per_bar}/{beat_unit}"
            );
            assert!(events[events_per_bar].1.is_accent);
        }
    }

    #[test]
    fn incomplete_pair_at_bar_end_stays_straight() {
        // A 3/4 bar contains one complete pair of quarter notes and one
        // unpaired quarter note. The third beat remains at its straight frame.
        let events = frames(3, 4, 1, SwingGrid::Quarter, 100, 4);
        assert_eq!(events, vec![0, 48_000, 48_000, 72_000]);
    }

    #[test]
    fn odd_subdivisions_are_piecewise_linearly_warped_without_drops() {
        let events = collect_events(
            120_000,
            4,
            4,
            5,
            SwingSettings::new(SwingGrid::Eighth, 100),
            21,
        );
        assert_eq!(events.len(), 21);
        assert_eq!(events[0].0, 0);
        assert_eq!(events[5].0, 24_000);
        assert_eq!(events[10].0, 48_000);
        assert_eq!(events[20].0, 96_000);
        assert!(events.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        for (ordinal, (_, event)) in events[..20].iter().enumerate() {
            assert_eq!(
                usize::from(event.beat_index) * 5 + usize::from(event.subdivision_index),
                ordinal
            );
        }
    }

    #[test]
    fn every_supported_subdivision_preserves_event_count_and_bar_length() {
        for subdivisions in 1..=8 {
            let count = 4 * usize::from(subdivisions) + 1;
            let events = collect_events(
                137_123,
                4,
                4,
                subdivisions,
                SwingSettings::new(SwingGrid::Sixteenth, -73),
                count,
            );
            assert_eq!(events.len(), count, "subdivision {subdivisions}");
            assert!(events.windows(2).all(|pair| pair[0].0 < pair[1].0));
            let expected_bar_frames =
                (u64::from(SAMPLE_RATE) * 60_000 * 4 * 4).div_ceil(u64::from(137_123_u32 * 4));
            assert_eq!(events[count - 1].0, expected_bar_frames);
        }
    }

    #[test]
    fn long_run_has_no_cumulative_bar_drift() {
        let bars = 1_000_u64;
        let beats_per_bar = 7_u8;
        let subdivisions = 7_u8;
        let events_per_bar = usize::from(beats_per_bar) * usize::from(subdivisions);
        let sample_rate = 1_000_u32;
        let mut scheduler = BeatScheduler::new(sample_rate);
        scheduler.start();
        let count = usize::try_from(bars).unwrap() * events_per_bar + 1;
        let mut events = Vec::with_capacity(count);
        for frame in 0..2_000_000_u64 {
            scheduler.advance_frame(
                120_000,
                beats_per_bar,
                8,
                subdivisions,
                SwingSettings::new(SwingGrid::Sixteenth, 100),
                |event| {
                    if events.len() < count {
                        events.push((frame, event));
                    }
                },
            );
            if events.len() == count {
                break;
            }
        }
        let expected = 250 * u64::from(beats_per_bar) * bars;
        assert_eq!(events.len(), count);
        assert_eq!(events.last().unwrap().0, expected);
        assert!(events.last().unwrap().1.is_accent);
    }

    #[test]
    fn amount_is_clamped_to_supported_range() {
        assert_eq!(
            frames(4, 4, 2, SwingGrid::Eighth, 999, 5),
            frames(4, 4, 2, SwingGrid::Eighth, 100, 5)
        );
        assert_eq!(
            frames(4, 4, 2, SwingGrid::Eighth, -999, 5),
            frames(4, 4, 2, SwingGrid::Eighth, -100, 5)
        );
    }

    #[test]
    fn beat_identity_and_accent_are_preserved() {
        let events = collect_events(
            120_000,
            4,
            4,
            4,
            SwingSettings::new(SwingGrid::Eighth, 67),
            5,
        );
        assert!(events[0].1.is_accent);
        for (index, (_, event)) in events[1..4].iter().enumerate() {
            assert_eq!(event.beat_index, 0);
            assert_eq!(event.subdivision_index, index as u8 + 1);
            assert!(!event.is_accent);
        }
        assert_eq!(events[4].1.beat_index, 1);
        assert_eq!(events[4].1.subdivision_index, 0);
        assert!(!events[4].1.is_accent);
    }
}
