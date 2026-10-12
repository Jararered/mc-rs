//! The sound queue the simulation writes to.

use bevy::prelude::*;
use game::block::blocks::Block;
use game::world::sound::MAX_QUEUED;
use game::world::sound::Pitch;
use game::world::sound::Sounds;

#[test]
fn a_sound_is_queued_as_it_was_asked_for() {
    let mut sounds = Sounds::default();
    sounds.play("random.click", Vec3::new(1.5, 64.5, -3.5), 0.3, 0.6);
    sounds.play_ui("random.click", 1.0, 1.0);
    let queued = sounds.queued();
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0].name, "random.click");
    assert_eq!(queued[0].position, Some(Vec3::new(1.5, 64.5, -3.5)));
    assert_eq!((queued[0].volume, queued[0].pitch), (0.3, 0.6));
    assert_eq!(queued[1].position, None, "a menu click has no place");
    assert_eq!(sounds.drain().count(), 2);
    assert!(sounds.queued().is_empty());
}

#[test]
fn a_varied_pitch_stays_inside_betas_range() {
    let mut sounds = Sounds::default();
    for _ in 0..200 {
        sounds.clear();
        sounds.play("mob.pig", Vec3::ZERO, 1.0, Pitch::VOICE);
        sounds.play("tile.piston.out", Vec3::ZERO, 0.5, Pitch::Up(0.6, 0.25));
        sounds.play("random.fizz", Vec3::ZERO, 0.5, Pitch::Around(2.6, 0.8));
        let pitches: Vec<f32> = sounds.queued().iter().map(|sound| sound.pitch).collect();
        assert!((0.8..=1.2).contains(&pitches[0]), "{}", pitches[0]);
        assert!((0.6..0.85).contains(&pitches[1]), "{}", pitches[1]);
        assert!((1.8..=3.4).contains(&pitches[2]), "{}", pitches[2]);
    }
}

#[test]
fn pitches_are_drawn_in_the_order_sounds_are_asked_for() {
    let mut first = Sounds::default();
    let mut second = Sounds::default();
    for sounds in [&mut first, &mut second] {
        sounds.play("mob.cow", Vec3::ZERO, 0.4, Pitch::VOICE);
        sounds.play("mob.cow", Vec3::ZERO, 0.4, Pitch::VOICE);
    }
    assert_eq!(first.queued(), second.queued());
    assert_ne!(first.queued()[0].pitch, first.queued()[1].pitch);
}

#[test]
fn a_silent_sound_is_dropped_and_the_queue_is_bounded() {
    let mut sounds = Sounds::default();
    sounds.play("random.splash", Vec3::ZERO, 0.0, 1.0);
    assert!(sounds.queued().is_empty());
    for _ in 0..MAX_QUEUED + 50 {
        sounds.play("random.pop", Vec3::ZERO, 0.2, 1.0);
    }
    assert_eq!(sounds.queued().len(), MAX_QUEUED);
}

#[test]
fn a_block_sounds_from_the_middle_of_its_cell() {
    let mut sounds = Sounds::default();
    let iron = Block::IronBlock.step_sound();
    sounds.play_block(iron.broken(), iron, IVec3::new(2, 70, -5));
    let sound = sounds.queued()[0];
    assert_eq!(sound.name, "step.stone");
    assert_eq!(sound.position, Some(Vec3::new(2.5, 70.5, -4.5)));
    // `(volume + 1) / 2` and four fifths of the pitch, which metal raises.
    assert_eq!(sound.volume, 1.0);
    assert!((sound.pitch - 1.2).abs() < 1e-6);
}

#[test]
fn a_splash_is_as_loud_as_the_entry_was_fast() {
    let mut sounds = Sounds::default();
    // Sideways speed counts for less than the fall.
    sounds.splash(Vec3::ZERO, Vec3::new(0.0, -1.0, 0.0));
    sounds.splash(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
    sounds.splash(Vec3::ZERO, Vec3::new(0.0, -20.0, 0.0));
    let volumes: Vec<f32> = sounds.queued().iter().map(|sound| sound.volume).collect();
    assert!((volumes[0] - 0.2).abs() < 1e-6);
    assert!((volumes[1] - 0.2 * 0.2f32.sqrt()).abs() < 1e-6);
    assert_eq!(volumes[2], 1.0);
    // A body that drifts in makes no sound at all.
    sounds.clear();
    sounds.splash(Vec3::ZERO, Vec3::ZERO);
    assert!(sounds.queued().is_empty());
}
