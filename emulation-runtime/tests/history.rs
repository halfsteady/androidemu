use emulation_runtime::history::Rewind;

#[test]
fn variable_sizes_failures_and_branching() {
    let mut history = Rewind::new(4096);
    history.push(&[1; 32]);
    history.push(&[2; 64]);
    history.push(&[3; 64]);
    assert_eq!(history.depth(), 2);
    assert_eq!(history.restore(|_| Err("rejected")), Err("rejected"));
    assert_eq!(history.depth(), 2);
    assert_eq!(
        history.restore(|state| {
            assert_eq!(state, &[2; 64]);
            Ok::<_, ()>(())
        }),
        Ok(true)
    );
    history.push(&[4; 16]);
    for expected in [vec![2; 64], vec![1; 32]] {
        assert_eq!(
            history.restore(|state| {
                assert_eq!(state, expected);
                Ok::<_, ()>(())
            }),
            Ok(true)
        );
    }
    assert_eq!(history.restore(|_| Ok::<_, ()>(())), Ok(false));
}

#[test]
fn memory_limit_and_oversized_snapshots() {
    let mut history = Rewind::new(2048);
    for frame in 0..1000 {
        history.push(&[frame as u8; 64]);
        assert!(history.bytes() <= 2048);
    }
    assert!(history.depth() > 0);
    history.push(&[0; 4096]);
    assert_eq!(history.depth(), 0);
    assert_eq!(history.bytes(), 0);
    history.push(&[7; 32]);
    history.push(&[8; 32]);
    assert_eq!(
        history.restore(|state| {
            assert_eq!(state, &[7; 32]);
            Ok::<_, ()>(())
        }),
        Ok(true)
    );
}

#[test]
fn changing_lengths_keep_sparse_deltas_instead_of_full_snapshots() {
    let mut history = Rewind::new(128 * 1024);
    let mut expected = Vec::new();
    for frame in 0..100 {
        let mut state = vec![0u8; 4096 + frame % 2];
        state[100] = frame as u8;
        history.push(&state);
        expected.push(state);
    }
    assert_eq!(history.depth(), 99);
    expected.pop();
    while let Some(expected) = expected.pop() {
        assert_eq!(
            history.restore(|actual| {
                assert_eq!(actual, expected);
                Ok::<_, ()>(())
            }),
            Ok(true)
        );
    }
}
