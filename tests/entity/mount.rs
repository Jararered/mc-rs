//! Rider yaw drift (`Entity.updateRidden`).

use game::entity::mount::rider_look_step;

#[test]
fn rider_look_takes_half_of_the_vehicle_turn_clamped_to_ten_degrees() {
    let mut yaw = 0.0;
    let mut pitch = 0.0;
    let (applied_yaw, applied_pitch) = rider_look_step(&mut yaw, &mut pitch, 40.0, 0.0);
    assert!((applied_yaw - 10.0).abs() < 1e-5);
    assert_eq!(applied_pitch, 0.0);
    // Half of 40° is 20°, clamped to 10°, so 30° remains.
    assert!((yaw - 30.0).abs() < 1e-5, "{yaw}");

    let (applied_yaw, _) = rider_look_step(&mut yaw, &mut pitch, 0.0, 0.0);
    assert!((applied_yaw - 10.0).abs() < 1e-5);
    assert!((yaw - 20.0).abs() < 1e-5, "{yaw}");
}
