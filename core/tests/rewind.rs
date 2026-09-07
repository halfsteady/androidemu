mod common;
use nes_core::{rewind::Rewind, Nes};

#[test]
fn the_chain_walks_back_through_every_state() {
    let mut rewind = Rewind::new(64 * 1024 * 1024);
    let states: Vec<Vec<u8>> = (0u8..40).map(|n| (0..500).map(|i| i as u8 ^ n.wrapping_mul(7)).collect()).collect();
    for state in &states {
        rewind.push(state);
    }
    assert_eq!(rewind.depth(), states.len() - 1);
    // The newest state is the anchor, so the chain replays the rest in reverse.
    for expected in states.iter().rev().skip(1) {
        assert_eq!(rewind.pop().unwrap(), &expected[..]);
    }
    assert!(rewind.pop().is_none());
}

#[test]
fn identical_and_wholly_changed_states_both_round_trip() {
    let mut rewind = Rewind::new(64 * 1024 * 1024);
    let quiet = vec![0xa5u8; 4096];
    let flipped: Vec<u8> = quiet.iter().map(|b| !b).collect();
    rewind.push(&quiet);
    rewind.push(&quiet);
    let unchanged = rewind.bytes();
    rewind.push(&flipped);
    // An untouched state costs a couple of bytes; a wholly changed one costs
    // its own length plus the run headers.
    assert!(unchanged < 8, "an unchanged frame cost {unchanged} bytes");
    assert!(rewind.bytes() > 4096);
    assert_eq!(rewind.pop().unwrap(), &quiet[..]);
    assert_eq!(rewind.pop().unwrap(), &quiet[..]);
    assert!(rewind.pop().is_none());
}

#[test]
fn the_budget_drops_the_oldest_and_keeps_the_newest_reachable() {
    // Room for a handful of deltas, each of which changes the whole buffer.
    let mut rewind = Rewind::new(4096);
    let states: Vec<Vec<u8>> = (0u8..30).map(|n| vec![n; 1000]).collect();
    for state in &states {
        rewind.push(state);
    }
    assert!(rewind.bytes() <= 4096);
    let reachable = rewind.depth();
    assert!(reachable > 0 && reachable < states.len() - 1, "kept {reachable} of {}", states.len());
    // Whatever survived must still be the states immediately before the newest.
    for expected in states.iter().rev().skip(1).take(reachable) {
        assert_eq!(rewind.pop().unwrap(), &expected[..]);
    }
    assert!(rewind.pop().is_none());
}

#[test]
fn a_different_game_restarts_the_chain() {
    let mut rewind = Rewind::new(64 * 1024 * 1024);
    rewind.push(&[1, 2, 3, 4]);
    rewind.push(&[5, 6, 7, 8]);
    assert_eq!(rewind.depth(), 1);
    rewind.push(&[9; 64]);
    assert_eq!(rewind.depth(), 0);
    assert!(rewind.pop().is_none());
}

#[test]
fn rewound_machine_states_load_and_replay_identically() {
    // The real thing: capture a frame at a time, wind back, and check every
    // state the chain hands over is one the machine actually had.
    let mut nes = Nes::new(&common::rom(4, 4)).unwrap();
    let mut rewind = Rewind::new(64 * 1024 * 1024);
    let mut expected = Vec::new();
    for _ in 0..30 {
        let state = nes.save_state();
        rewind.push(&state);
        expected.push(state);
        nes.step_frame();
    }
    for state in expected.iter().rev().skip(1) {
        let recovered = rewind.pop().expect("chain ran out early").to_vec();
        assert_eq!(&recovered, state);
        nes.load_state(&recovered).expect("a rewound state must load");
        assert_eq!(nes.save_state(), recovered);
    }
}

#[test]
fn frames_of_a_running_game_stay_affordable() {
    // A guard on the encoding, not a promise about any particular game: a
    // second of this ROM must not cost anything like a second of raw snapshots.
    let mut nes = Nes::new(&common::rom(4, 4)).unwrap();
    let mut rewind = Rewind::new(64 * 1024 * 1024);
    let snapshot = nes.save_state().len();
    for _ in 0..60 {
        rewind.push(&nes.save_state());
        nes.step_frame();
    }
    assert!(
        rewind.bytes() < snapshot * 60 / 4,
        "60 frames cost {} bytes against {} raw",
        rewind.bytes(),
        snapshot * 60
    );
}
