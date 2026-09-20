use game::player::HeartFill;
use game::player::MAX_PLAYER_HEALTH;
use game::player::PlayerHealth;

#[test]
fn default_health_is_ten_full_hearts() {
    let health = PlayerHealth::default();
    assert_eq!(health.current, MAX_PLAYER_HEALTH);
    for index in 0..10 {
        assert_eq!(health.heart_fill(index), HeartFill::Full);
    }
}

#[test]
fn hearts_show_half_and_empty_from_remaining_health() {
    let health = PlayerHealth { current: 13 };
    assert_eq!(health.heart_fill(0), HeartFill::Full);
    assert_eq!(health.heart_fill(5), HeartFill::Full);
    assert_eq!(health.heart_fill(6), HeartFill::Half);
    assert_eq!(health.heart_fill(7), HeartFill::Empty);
    assert_eq!(health.heart_fill(9), HeartFill::Empty);
}

#[test]
fn zero_health_is_all_empty_hearts() {
    let health = PlayerHealth { current: 0 };
    for index in 0..10 {
        assert_eq!(health.heart_fill(index), HeartFill::Empty);
    }
}
