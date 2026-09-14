//! One control runs time in both directions: drag left of centre to go back,
//! right to go forward, and the further from centre the faster it runs. Kept
//! pure because it is the part that has to feel right. Ported from Scrub.kt.

/// Frames per displayed frame at the far end of the track.
pub const MAX: i32 = 8;

/// The middle of the track is dead: a pointer resting slightly off centre
/// must not creep the game along.
pub const DEAD_ZONE: f32 = 0.12;

/// `fraction` runs -1 (fully left) to 1 (fully right). Negative results are
/// frames to step back per displayed frame, positive frames to run forward,
/// zero is ordinary play.
pub fn speed(fraction: f32) -> i32 {
    let magnitude = fraction.abs().min(1.0);
    if magnitude <= DEAD_ZONE {
        return 0;
    }
    let past = (magnitude - DEAD_ZONE) / (1.0 - DEAD_ZONE);
    let steps = ((past * MAX as f32).ceil() as i32).clamp(1, MAX);
    if fraction < 0.0 {
        -steps
    } else {
        steps
    }
}

/// What the heads-up display says while the track is held.
pub fn label(speed: i32) -> String {
    match speed {
        s if s < 0 => format!("Rewinding {}×", -s),
        s if s > 0 => format!("Fast-forward {s}×"),
        _ => "Ready".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_of_the_track_is_stopped() {
        assert_eq!(speed(0.0), 0);
        assert_eq!(speed(DEAD_ZONE), 0);
        assert_eq!(speed(-DEAD_ZONE), 0);
        assert_eq!(speed(DEAD_ZONE + 0.001), 1);
        assert_eq!(speed(-DEAD_ZONE - 0.001), -1);
    }

    #[test]
    fn the_ends_of_the_track_are_the_fastest() {
        assert_eq!(speed(1.0), MAX);
        assert_eq!(speed(-1.0), -MAX);
        assert_eq!(speed(3.0), MAX);
        assert_eq!(speed(-3.0), -MAX);
    }

    #[test]
    fn speed_rises_with_distance_and_never_skips_backwards() {
        let mut previous = 0;
        for step in 0..=100 {
            let at = step as f32 / 100.0;
            let s = speed(at);
            assert!(s >= previous, "{at} went from {previous} to {s}");
            assert!((0..=MAX).contains(&s));
            assert_eq!(speed(-at), -s);
            previous = s;
        }
        assert_eq!(previous, MAX);
    }

    #[test]
    fn every_step_between_one_and_the_maximum_is_reachable() {
        let reached: std::collections::BTreeSet<i32> =
            (0..=100).map(|i| speed(i as f32 / 100.0)).collect();
        assert_eq!(reached, (0..=MAX).collect());
    }

    #[test]
    fn the_label_says_which_way_time_is_going() {
        assert_eq!(label(0), "Ready");
        assert_eq!(label(-3), "Rewinding 3×");
        assert_eq!(label(8), "Fast-forward 8×");
    }
}
