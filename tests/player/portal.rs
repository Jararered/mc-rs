use game::player::portal::PortalTravel;

#[test]
fn after_arrival_clears_charge_and_uses_portal_cooldown() {
    let arrived = PortalTravel::after_arrival();
    assert_eq!(arrived.cooldown(), 10);
    assert!(!arrived.touching());
    assert!(!arrived.charging());
    assert_eq!(arrived.strength(0.0), 0.0);
    assert_eq!(arrived.strength(1.0), 0.0);

    // Spawning uses the longer cooldown; a portal trip must not.
    assert_eq!(PortalTravel::default().cooldown(), 20);
}

#[test]
fn completing_a_charge_leaves_full_strength_that_after_arrival_clears() {
    let mut portal = PortalTravel::default();
    for _ in 0..portal.cooldown() {
        assert!(!portal.tick());
    }
    let mut travelled = false;
    for _ in 0..80 {
        portal.set_in_portal();
        if portal.tick() {
            travelled = true;
            break;
        }
    }
    assert!(travelled);
    assert_eq!(portal.strength(0.0), 1.0);
    assert!(portal.cooldown() > 0);

    let arrived = PortalTravel::after_arrival();
    assert_eq!(arrived.strength(0.0), 0.0);
    assert_eq!(arrived.cooldown(), 10);
}
