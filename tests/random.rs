use game::random::ItemRng;
use game::random::JavaRandom;

#[test]
fn java_random_matches_the_reference_sequence_for_mixed_draws() {
    let mut integer = JavaRandom::new(0);
    assert_eq!(integer.next_int(100), 60);

    let mut double = JavaRandom::new(0);
    assert_eq!(double.next_double(), 0.730_967_787_376_657);

    let mut float = JavaRandom::new(0);
    assert_eq!(float.next_float(), 0.730_967_76);

    let mut long = JavaRandom::new(0);
    assert_eq!(long.next_long(), -4_962_768_465_676_381_896);
}

#[test]
fn restoring_a_java_random_state_continues_the_same_sequence() {
    let mut random = JavaRandom::new(42);
    for _ in 0..20 {
        random.next_int(997);
    }
    let mut restored = JavaRandom::from_state(random.state());
    for _ in 0..100 {
        assert_eq!(random.next_bits(32), restored.next_bits(32));
    }
}

#[test]
fn bounded_java_integers_stay_in_range_for_power_and_non_power_of_two_bounds() {
    for bound in [1, 2, 16, 31, 100, 1_000_003] {
        let mut random = JavaRandom::new(u64::from(bound));
        for _ in 0..10_000 {
            assert!(random.next_int(bound) < bound);
        }
    }
}

#[test]
fn java_float_and_item_rng_draws_are_unit_interval_and_repeatable() {
    let mut java = JavaRandom::new(1234);
    for _ in 0..10_000 {
        let value = java.next_float();
        assert!((0.0..1.0).contains(&value));
    }

    let mut left = ItemRng::default();
    let mut right = ItemRng::default();
    for _ in 0..10_000 {
        let value = left.unit();
        assert!((0.0..1.0).contains(&value));
        assert_eq!(value, right.unit());
    }

    let mut left = ItemRng::default();
    let mut right = ItemRng::default();
    for _ in 0..100 {
        assert_eq!(left.next_u64(), right.next_u64());
    }
}
