use super::*;

#[test]
fn a_stream_at_a_position_continues_like_the_original() {
    let key = StreamKey::derive(5, 9, "actor");
    for n in [0u64, 1, 7, 8, 15, 16, 17, 100] {
        let mut original = Stream::new(key);
        for _ in 0..n {
            original.next_u64();
        }
        let mut at = Stream::at(key, n);
        assert_eq!(at, original);
        for _ in 0..40 {
            assert_eq!(at.next_u64(), original.next_u64(), "after {n} draws");
        }
    }
}

/// RFC 7539 §2.3.2: the ChaCha20 block with the test key, counter 1, nonce 0 — restricted here
/// to a zero nonce by using the keystream of §2.4.2's structure: we check the block function
/// against the RFC's §2.3.2 vector by setting the nonce words to the vector's values.
#[test]
fn chacha20_block_matches_rfc_7539() {
    let key: [u32; 8] = [
        0x03020100, 0x07060504, 0x0b0a0908, 0x0f0e0d0c, 0x13121110, 0x17161514, 0x1b1a1918,
        0x1f1e1d1c,
    ];
    // The RFC vector uses counter 1 and nonce 00:00:00:09:00:00:00:4a:00:00:00:00; our block
    // function has a zero nonce, so build the state by hand to compare the round function.
    let mut s = [
        0x61707865, 0x3320646e, 0x79622d32, 0x6b206574, key[0], key[1], key[2], key[3], key[4],
        key[5], key[6], key[7], 1, 0x09000000, 0x4a000000, 0,
    ];
    let init = s;
    for _ in 0..10 {
        quarter(&mut s, 0, 4, 8, 12);
        quarter(&mut s, 1, 5, 9, 13);
        quarter(&mut s, 2, 6, 10, 14);
        quarter(&mut s, 3, 7, 11, 15);
        quarter(&mut s, 0, 5, 10, 15);
        quarter(&mut s, 1, 6, 11, 12);
        quarter(&mut s, 2, 7, 8, 13);
        quarter(&mut s, 3, 4, 9, 14);
    }
    for (w, i) in s.iter_mut().zip(init) {
        *w = w.wrapping_add(i);
    }
    let expected: [u32; 16] = [
        0xe4e7f110, 0x15593bd1, 0x1fdd0f50, 0xc47120a3, 0xc7f4d1c7, 0x0368c033, 0x9aaa2204,
        0x4e6cd4c3, 0x466482d2, 0x09aa9f07, 0x05d7c214, 0xa2028bd9, 0xd19c12b5, 0xb94e16de,
        0xe883d0cb, 0x4e3c50a2,
    ];
    assert_eq!(s, expected);
}

#[test]
fn stream_is_reproducible_and_independent() {
    let mut a = Stream::for_actor(42, 1);
    let mut b = Stream::for_actor(42, 1);
    let mut c = Stream::for_actor(42, 2);
    let mut d = Stream::for_actor(43, 1);
    let xs: Vec<u64> = (0..40).map(|_| a.next_u64()).collect();
    let ys: Vec<u64> = (0..40).map(|_| b.next_u64()).collect();
    assert_eq!(xs, ys);
    assert_ne!(xs[0], c.next_u64());
    assert_ne!(xs[0], d.next_u64());
    assert_eq!(a.drawn(), 40);
    // Rollback: a copy taken before draws restores the position.
    let saved = a;
    let after = a.next_u64();
    a = saved;
    assert_eq!(a.next_u64(), after);
}

#[test]
fn purpose_labels_give_independent_streams() {
    assert_ne!(
        StreamKey::derive(1, 1, "actor"),
        StreamKey::derive(1, 1, "connection")
    );
}

#[test]
fn a_position_beyond_any_run_does_not_overflow() {
    let key = StreamKey::derive(1, 2, "actor");
    let s = Stream::at(key, u64::MAX);
    assert_eq!(s.drawn(), u64::MAX);
    let t = Stream::at(key, 5);
    let mut u = Stream::new(key);
    for _ in 0..5 {
        u.next_u64();
    }
    assert_eq!(t, u);
}
