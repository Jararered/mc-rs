//! Sound: which files make up which sound, how loud and on which side one is
//! heard, and when music plays. Nothing here opens an audio device, and
//! nothing needs the `resources/` folder to be there.

use std::collections::HashSet;
use std::fs;
use std::num::NonZero;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bevy::audio::ChannelCount;
use bevy::audio::Sample;
use bevy::audio::SampleRate;
use bevy::audio::Source;
use bevy::prelude::*;
use game::audio::library::RESOURCES_DIR;
use game::audio::library::Scan;
use game::audio::library::pool_name;
use game::audio::library::read_source;
use game::audio::library::scan;
use game::audio::music::MUSIC_DELAY_TICKS;
use game::audio::music::MusicTimer;
use game::audio::playback::ChannelGains;
use game::audio::playback::HEARING_RANGE;
use game::audio::playback::Listener;
use game::audio::playback::Stereo;
use game::audio::playback::UI_VOLUME;
use game::audio::playback::attenuation;
use game::audio::playback::channel_gains;
use game::audio::playback::hearing_range;
use game::block::definition::StepSound;
use game::entity::mobs::MobType;
use game::entity::voice::voice;

use crate::temp::TempDir;

fn temp_folder(label: &str) -> TempDir {
    TempDir::new(&format!("sounds-{label}"))
}

fn touch(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"").unwrap();
}

#[test]
fn pool_names_follow_sound_pool() {
    // The variants of one sound share a name.
    assert_eq!(pool_name("step/grass1.ogg").as_deref(), Some("step.grass"));
    assert_eq!(pool_name("step/grass4.ogg").as_deref(), Some("step.grass"));
    assert_eq!(
        pool_name("random/click.ogg").as_deref(),
        Some("random.click")
    );
    assert_eq!(
        pool_name("mob/zombiepig/zpigangry4.ogg").as_deref(),
        Some("mob.zombiepig.zpigangry")
    );
    // Beta asks for `mob.ghast.fireball`, and the only file is numbered 4.
    assert_eq!(
        pool_name("mob/ghast/fireball4.ogg").as_deref(),
        Some("mob.ghast.fireball")
    );
    // A folder and a file may share a stem: these are two pools.
    assert_eq!(pool_name("mob/zombie1.ogg").as_deref(), Some("mob.zombie"));
    assert_eq!(
        pool_name("mob/zombie/metal1.ogg").as_deref(),
        Some("mob.zombie.metal")
    );
    assert_eq!(
        pool_name("random/wood click.ogg").as_deref(),
        Some("random.wood click")
    );
    // `SoundPool.addSound` throws on these; they are skipped.
    assert_eq!(pool_name("README"), None);
    assert_eq!(pool_name(".DS_Store"), None);
    assert_eq!(pool_name("13.ogg"), None);
}

#[test]
fn scan_sorts_the_folder_into_beta_pools() {
    let root = temp_folder("scan");
    for file in [
        "sound/step/grass1.ogg",
        "newsound/step/grass1.ogg",
        "newsound/step/grass2.ogg",
        "newsound/mob/zombie1.ogg",
        "newsound/mob/zombie/metal1.ogg",
        "newsound/random/wood click.ogg",
        "music/calm1.ogg",
        "newmusic/hal2.ogg",
        "streaming/13.mus",
        // Not Ogg files.
        "newsound/.DS_Store",
        "newsound/note/harp.txt",
        "streaming/13.ogg",
        // Folders Beta does not read.
        "sound3/dig/stone1.ogg",
        "icons/icon.ogg",
    ] {
        touch(&root, file);
    }
    let found = scan(&root);
    let names: Vec<&str> = found
        .effects
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "step.grass",
            "mob.zombie.metal",
            "mob.zombie",
            "random.wood click",
            "step.grass",
            "step.grass",
        ]
    );
    assert_eq!(
        found.music,
        [root.join("music/calm1.ogg"), root.join("newmusic/hal2.ogg")]
    );
    // The streaming pool keeps a name's digits, and reads `.mus` too.
    assert_eq!(
        found.records,
        [
            ("13".to_string(), root.join("streaming/13.mus")),
            ("13".to_string(), root.join("streaming/13.ogg")),
        ]
    );
}

#[test]
fn a_missing_folder_is_an_empty_library() {
    assert_eq!(
        scan(&temp_folder("missing").join("resources")),
        Scan::default()
    );
}

#[test]
fn a_file_that_is_not_ogg_is_refused() {
    let root = temp_folder("refused");
    touch(&root, "empty.ogg");
    fs::write(root.join("text.ogg"), b"RIFF, or anything but Ogg").unwrap();
    assert!(read_source(&root.join("empty.ogg")).is_none());
    assert!(read_source(&root.join("text.ogg")).is_none());
    assert!(read_source(&root.join("absent.ogg")).is_none());
}

#[test]
fn a_sound_fades_evenly_to_nothing_at_its_range() {
    assert_eq!(hearing_range(1.0), HEARING_RANGE);
    assert_eq!(hearing_range(0.3), HEARING_RANGE);
    assert_eq!(attenuation(0.0, 1.0), 1.0);
    assert_eq!(attenuation(8.0, 1.0), 0.5);
    assert_eq!(attenuation(16.0, 1.0), 0.0);
    assert_eq!(attenuation(40.0, 1.0), 0.0);
    // A quieter sound starts lower and reaches the same distance.
    assert!((attenuation(8.0, 0.3) - 0.15).abs() < 1e-6);
    // A volume above one is no louder at the source; it carries further.
    assert_eq!(hearing_range(4.0), 64.0);
    assert_eq!(attenuation(0.0, 4.0), 1.0);
    assert_eq!(attenuation(32.0, 4.0), 0.5);
    assert_eq!(attenuation(64.0, 4.0), 0.0);
}

/// A listener at the origin looking down -Z, as a fresh Bevy camera does.
const AHEAD: Listener = Listener {
    position: Vec3::ZERO,
    right: Vec3::X,
};

#[test]
fn a_sound_is_louder_in_the_ear_it_is_nearer() {
    let [left, right] = channel_gains(Some(AHEAD), Some(Vec3::new(4.0, 0.0, 0.0)), 1.0);
    assert!(left.abs() < 1e-6, "all of it is on the right: {left}");
    assert!((right - 0.75).abs() < 1e-6);
    let [left, right] = channel_gains(Some(AHEAD), Some(Vec3::new(-4.0, 0.0, 0.0)), 1.0);
    assert!((left - 0.75).abs() < 1e-6);
    assert!(right.abs() < 1e-6);
    // Straight ahead, overhead and underfoot are in the middle.
    for middle in [Vec3::NEG_Z * 4.0, Vec3::Y * 4.0, Vec3::NEG_Y * 1.62] {
        let [left, right] = channel_gains(Some(AHEAD), Some(middle), 1.0);
        assert!((left - right).abs() < 1e-6, "{middle}");
        assert!(left > 0.0);
    }
    // Ahead and to the right is in both ears, the right the louder.
    let [left, right] = channel_gains(Some(AHEAD), Some(Vec3::new(3.0, 0.0, -3.0)), 1.0);
    assert!(right > left && left > 0.0);
}

#[test]
fn a_sound_at_the_listener_or_out_of_range_has_no_side() {
    let [left, right] = channel_gains(Some(AHEAD), Some(Vec3::ZERO), 1.0);
    assert_eq!(left, right);
    assert!((left - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    assert_eq!(
        channel_gains(Some(AHEAD), Some(Vec3::X * 16.0), 1.0),
        [0.0; 2]
    );
    // With nobody in the world to hear it, a sound there is silent.
    assert_eq!(channel_gains(None, Some(Vec3::ZERO), 1.0), [0.0; 2]);
}

#[test]
fn a_sound_with_no_place_plays_in_the_middle_at_a_quarter() {
    let expected = UI_VOLUME * std::f32::consts::FRAC_1_SQRT_2;
    for listener in [None, Some(AHEAD)] {
        let [left, right] = channel_gains(listener, None, 1.0);
        assert_eq!(left, right);
        assert!((left - expected).abs() < 1e-6);
    }
    // `playSoundFX` clamps the volume before quartering it.
    assert_eq!(
        channel_gains(None, None, 3.0),
        channel_gains(None, None, 1.0)
    );
}

#[test]
fn the_listener_turns_with_the_head_and_stays_level() {
    // Facing +X, the right ear points along +Z.
    let turned = Transform::from_xyz(3.0, 70.0, -2.0).looking_to(Vec3::X, Vec3::Y);
    let listener = Listener::of(&GlobalTransform::from(turned));
    assert_eq!(listener.position, Vec3::new(3.0, 70.0, -2.0));
    assert!(listener.right.distance(Vec3::Z) < 1e-5);
    // Looking down does not tip the ears.
    let down = Transform::IDENTITY.looking_to(Vec3::new(1.0, -5.0, 0.0), Vec3::Y);
    let listener = Listener::of(&GlobalTransform::from(down));
    assert!(listener.right.distance(Vec3::Z) < 1e-5);
}

/// A decoded file standing in for the Ogg decoder.
struct Samples {
    samples: std::vec::IntoIter<Sample>,
    channels: u16,
}

impl Samples {
    fn new(channels: u16, samples: &[Sample]) -> Self {
        Self {
            samples: Vec::from(samples).into_iter(),
            channels,
        }
    }
}

impl Iterator for Samples {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        self.samples.next()
    }
}

impl Source for Samples {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(self.channels).unwrap()
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(22050).unwrap()
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[test]
fn a_mono_file_plays_on_both_channels_at_their_gains() {
    let gains = Arc::new(ChannelGains::new([0.5, 0.25]));
    let stereo = Stereo::new(Samples::new(1, &[1.0, -0.5]), gains);
    assert_eq!(stereo.channels().get(), 2);
    assert_eq!(stereo.sample_rate().get(), 22050);
    let played: Vec<Sample> = stereo.collect();
    assert_eq!(played, [0.5, 0.25, -0.25, -0.125]);
}

#[test]
fn a_stereo_file_is_mixed_down_before_it_is_placed() {
    let gains = Arc::new(ChannelGains::new([1.0, 0.0]));
    let stereo = Stereo::new(Samples::new(2, &[1.0, 0.0, 0.25, 0.75]), gains);
    let played: Vec<Sample> = stereo.collect();
    assert_eq!(played, [0.5, 0.0, 0.5, 0.0]);
}

#[test]
fn a_change_of_gain_is_eased_in() {
    let gains = Arc::new(ChannelGains::new([0.0, 0.0]));
    let mut stereo = Stereo::new(Samples::new(1, &[1.0; 4000]), Arc::clone(&gains));
    assert_eq!(stereo.next(), Some(0.0));
    gains.set([1.0, 1.0]);
    let played: Vec<Sample> = stereo.collect();
    // The first frame after the change has barely moved; the gain climbs
    // without a step and is all but there within a tenth of a second.
    assert_eq!(played[0], 0.0, "the right half of the frame already begun");
    assert!(played[1] > 0.0 && played[1] < 0.01);
    assert!(played.windows(2).all(|pair| pair[1] >= pair[0]));
    assert!(*played.last().unwrap() > 0.99);
}

#[test]
fn music_waits_before_the_first_track_and_longer_between_tracks() {
    for seed in 0..20 {
        let mut timer = MusicTimer::new(seed);
        let wait = timer.ticks_before_music();
        assert!(wait < MUSIC_DELAY_TICKS, "up to ten minutes: {wait}");
        for _ in 0..wait {
            assert!(!timer.tick());
        }
        assert!(timer.tick(), "the track is due when the wait is over");
        let next = timer.ticks_before_music();
        assert!(
            (MUSIC_DELAY_TICKS..2 * MUSIC_DELAY_TICKS).contains(&next),
            "ten to twenty minutes: {next}"
        );
        assert!(!timer.tick());
    }
}

/// Every sound name the game can ask for: the block and mob tables, and the
/// names written out in the sources.
fn names_the_game_plays() -> HashSet<String> {
    let mut names = HashSet::new();
    for sound in [
        StepSound::Stone,
        StepSound::Wood,
        StepSound::Gravel,
        StepSound::Grass,
        StepSound::Metal,
        StepSound::Glass,
        StepSound::Cloth,
        StepSound::Sand,
    ] {
        names.insert(sound.step().to_string());
        names.insert(sound.broken().to_string());
    }
    for kind in MobType::ALL {
        let voice = voice(kind);
        names.extend(
            [voice.living, voice.hurt, voice.death]
                .into_iter()
                .flatten()
                .map(str::to_string),
        );
    }
    let mut sources = Vec::new();
    collect_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut sources,
    );
    for source in sources {
        for line in fs::read_to_string(source).unwrap().lines() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            // Every other piece of a line split on quotes is a string.
            names.extend(
                line.split('"')
                    .skip(1)
                    .step_by(2)
                    .filter(|text| looks_like_a_sound(text))
                    .map(str::to_string),
            );
        }
    }
    names
}

fn collect_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

fn looks_like_a_sound(text: &str) -> bool {
    const POOLS: [&str; 9] = [
        "ambient.", "fire.", "liquid.", "mob.", "note.", "portal.", "random.", "step.", "tile.",
    ];
    POOLS
        .iter()
        .any(|pool| text.starts_with(pool) && text.len() > pool.len())
        && text
            .chars()
            .all(|letter| letter.is_ascii_lowercase() || matches!(letter, '.' | '_' | ' '))
}

#[test]
fn the_sources_name_the_sounds_beta_plays() {
    let names = names_the_game_plays();
    // A spot check that the source scan finds what is there.
    for name in [
        "random.click",
        "random.explode",
        "mob.chickenplop",
        "tile.piston.out",
        "ambient.cave.cave",
        "portal.travel",
        "note.bassattack",
        "step.cloth",
        "mob.zombiepig.zpigangry",
    ] {
        assert!(names.contains(name), "{name} is played somewhere");
    }
}

/// With the player's own `resources/` folder present, every name the game
/// asks for is the name of at least one file in it. A misspelt name would
/// otherwise only show as a sound that never plays.
#[test]
fn every_sound_asked_for_has_a_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(RESOURCES_DIR);
    let found = scan(&root);
    if found.effects.is_empty() {
        eprintln!("no sounds under {}; nothing to check", root.display());
        return;
    }
    let pools: HashSet<&str> = found
        .effects
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let mut missing: Vec<String> = names_the_game_plays()
        .into_iter()
        .filter(|name| !pools.contains(name.as_str()))
        .collect();
    missing.sort();
    assert!(missing.is_empty(), "no file for {missing:?}");
    assert!(
        !found.music.is_empty(),
        "music/ and newmusic/ hold the tracks"
    );
}

/// The whole plugin beside a headless world, as the game runs it. With no
/// output device Bevy plays nothing and the rest carries on; either way
/// every system here has to accept its parameters and run.
#[test]
fn the_sound_plugin_runs_beside_a_world() {
    use bevy::asset::AssetPlugin;
    use bevy::audio::AudioPlugin;
    use game::app::settings::GameSettings;
    use game::audio::SoundPlugin;
    use game::world::plugin::WorldPlugin;
    use game::world::sound::Sounds;
    use game::world::tick::WorldTick;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        AudioPlugin::default(),
        WorldPlugin,
        SoundPlugin,
    ))
    .init_resource::<GameSettings>();
    for _ in 0..3 {
        app.world_mut().resource_mut::<WorldTick>().advance(0.05);
        // A name with no file behind it is asked for and dropped.
        app.world_mut()
            .resource_mut::<Sounds>()
            .play("test.silence", Vec3::ZERO, 1.0, 1.0);
        app.update();
        assert!(app.world().resource::<Sounds>().queued().is_empty());
    }
}

#[test]
fn without_bevys_audio_the_sound_plugin_adds_nothing() {
    use game::audio::SoundPlugin;
    use game::world::sound::Sounds;

    let mut app = App::new();
    app.add_plugins((MinimalPlugins, SoundPlugin));
    app.update();
    assert!(!app.world().contains_resource::<Sounds>());
}

/// A listening test for a person, and a check that a real file plays through
/// a real device: with `resources/` in place, a click sounds on the right
/// and then on the left, and each is gone when it has finished.
///
/// ```sh
/// cargo test -- --ignored a_click_plays_in_each_ear --nocapture
/// ```
#[test]
#[ignore = "plays sound through the default output device"]
fn a_click_plays_in_each_ear() {
    use bevy::asset::AssetPlugin;
    use bevy::audio::AudioPlugin;
    use bevy::audio::AudioSink;
    use game::app::settings::GameSettings;
    use game::audio::SoundPlugin;
    use game::audio::library::SoundLibrary;
    use game::audio::playback::Emitter;
    use game::player::PlayerCamera;
    use game::world::plugin::WorldPlugin;
    use game::world::sound::Sounds;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        AudioPlugin::default(),
        WorldPlugin,
        SoundPlugin,
    ))
    .init_resource::<GameSettings>();
    // The listener, looking down -Z from the origin.
    app.world_mut().spawn((PlayerCamera, Transform::default()));
    let frame = Duration::from_millis(10);
    let wait = |app: &mut App, what: &str, done: &dyn Fn(&mut World) -> bool| {
        for _ in 0..300 {
            app.update();
            if done(app.world_mut()) {
                return;
            }
            std::thread::sleep(frame);
        }
        panic!("{what} did not happen within three seconds");
    };
    wait(&mut app, "loading the sounds", &|world| {
        world.resource::<SoundLibrary>().has("random.click")
    });
    let playing = |world: &mut World| {
        world
            .query_filtered::<(), (With<Emitter>, With<AudioSink>)>()
            .iter(world)
            .count()
    };
    let emitters = |world: &mut World| {
        world
            .query_filtered::<(), With<Emitter>>()
            .iter(world)
            .count()
    };
    for side in [4.0, -4.0] {
        app.world_mut().resource_mut::<Sounds>().play(
            "random.click",
            Vec3::new(side, 0.0, 0.0),
            1.0,
            1.0,
        );
        wait(&mut app, "the click starting", &|world| playing(world) == 1);
        wait(&mut app, "the click finishing", &|world| {
            emitters(world) == 0
        });
        std::thread::sleep(Duration::from_millis(300));
    }
}

/// `MusInputStream` run backwards: what the launcher did to make a `.mus`.
fn scramble_mus(file_name: &str, bytes: &mut [u8]) {
    let mut hash = file_name.encode_utf16().fold(0i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    });
    for byte in bytes {
        let plain = *byte;
        *byte ^= (hash >> 8) as u8;
        hash = hash
            .wrapping_mul(498_729_871)
            .wrapping_add(85731_i32.wrapping_mul(i32::from(plain as i8)));
    }
}

#[test]
fn a_mus_file_unscrambles_by_its_own_name() {
    use game::audio::library::unscramble_mus;
    let plain: Vec<u8> = (0..=255u8).cycle().take(2000).collect();
    let mut bytes = plain.clone();
    scramble_mus("cat.mus", &mut bytes);
    assert_ne!(bytes, plain);
    let mut wrong = bytes.clone();
    unscramble_mus("cat.mus", &mut bytes);
    assert_eq!(bytes, plain);
    // The key is the file's name: under another it stays noise.
    unscramble_mus("13.mus", &mut wrong);
    assert_ne!(wrong, plain);
}

#[test]
fn a_record_carries_four_times_as_far_at_half_the_volume() {
    use game::audio::playback::channel_gains_within;
    use game::audio::records::RECORD_RANGE;
    use game::audio::records::RECORD_VOLUME;
    assert_eq!(RECORD_RANGE, 64.0);
    let at = |distance: f32| {
        let [left, right] = channel_gains_within(
            Some(AHEAD),
            Some(Vec3::NEG_Z * distance),
            RECORD_VOLUME,
            RECORD_RANGE,
        );
        assert!((left - right).abs() < 1e-6);
        left / std::f32::consts::FRAC_1_SQRT_2
    };
    assert!((at(0.0) - 0.5).abs() < 1e-6);
    assert!((at(32.0) - 0.25).abs() < 1e-6);
    assert_eq!(at(64.0), 0.0);
    // An ordinary sound of that volume would be gone at 16.
    assert_eq!(
        channel_gains(Some(AHEAD), Some(Vec3::NEG_Z * 16.0), 0.5),
        [0.0; 2]
    );
}

#[test]
fn a_started_record_is_named_over_the_hotbar_for_three_seconds() {
    use game::audio::records::NowPlaying;
    use game::ui::screens::hud::record_line_color;
    let mut now_playing = NowPlaying::default();
    now_playing.announce("cat");
    assert_eq!(now_playing.text, "Now playing: C418 - cat");
    assert_eq!(now_playing.ticks, 60);
    // Solid until the last second, then fading out to nothing.
    assert_eq!(record_line_color(60.0).unwrap().alpha(), 1.0);
    assert_eq!(record_line_color(20.0).unwrap().alpha(), 1.0);
    let fading = record_line_color(10.0).unwrap().alpha();
    assert!((fading - 0.5).abs() < 0.01, "{fading}");
    assert!(record_line_color(0.0).is_none());
}

/// With the player's `resources/` in place: both of Beta's discs are there,
/// by the names the item table gives them, and every file of each decodes,
/// the scrambled ones included.
#[test]
fn both_discs_have_a_track_that_decodes() {
    use game::item::Item;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(RESOURCES_DIR);
    let found = scan(&root);
    if found.records.is_empty() {
        eprintln!("no records under {}; nothing to check", root.display());
        return;
    }
    for disc in [Item::Record13, Item::RecordCat] {
        let name = disc.record_name().expect("a disc names its track");
        let files: Vec<&PathBuf> = found
            .records
            .iter()
            .filter(|(record, _)| record == name)
            .map(|(_, path)| path)
            .collect();
        assert!(!files.is_empty(), "no file for the record {name}");
        for file in files {
            assert!(read_source(file).is_some(), "{} decodes", file.display());
        }
    }
    assert_eq!(Item::Record13.record_name(), Some("13"));
    assert_eq!(Item::RecordCat.record_name(), Some("cat"));
    assert_eq!(Item::Diamond.record_name(), None);
}

/// A check through the real device that a disc starts and stops: with
/// `resources/` in place, aux effect 1005 for the cat disc gets a record
/// playing from a jukebox far enough off to be faint, and 1005 with no disc
/// ends it.
///
/// ```sh
/// cargo test -- --ignored a_jukebox_starts_and_stops_a_record
/// ```
#[test]
#[ignore = "plays sound through the default output device"]
fn a_jukebox_starts_and_stops_a_record() {
    use bevy::asset::AssetPlugin;
    use bevy::audio::AudioPlugin;
    use bevy::audio::AudioSink;
    use game::app::settings::GameSettings;
    use game::audio::SoundPlugin;
    use game::audio::records::NowPlaying;
    use game::audio::records::Record;
    use game::item::Item;
    use game::player::PlayerCamera;
    use game::world::block_ticks::AuxEffect;
    use game::world::plugin::WorldPlugin;

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        AudioPlugin::default(),
        WorldPlugin,
        SoundPlugin,
    ))
    .init_resource::<GameSettings>();
    app.world_mut().spawn((PlayerCamera, Transform::default()));
    let playing = |world: &mut World| {
        world
            .query_filtered::<(), (With<Record>, With<AudioSink>)>()
            .iter(world)
            .count()
    };
    let records = |world: &mut World| {
        world
            .query_filtered::<(), With<Record>>()
            .iter(world)
            .count()
    };
    let send = |app: &mut App, data: i32, done: &dyn Fn(&mut World) -> bool| {
        for attempt in 0..400 {
            // The library may still be loading: ask again until it answers.
            if attempt % 20 == 0 {
                app.world_mut()
                    .resource_mut::<Messages<AuxEffect>>()
                    .write(AuxEffect {
                        position: IVec3::new(60, 0, 0),
                        effect: 1005,
                        data,
                    });
            }
            app.update();
            if done(app.world_mut()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the jukebox did not answer within four seconds");
    };
    send(&mut app, i32::from(Item::RecordCat.as_u16()), &|world| {
        playing(world) == 1
    });
    assert_eq!(
        app.world().resource::<NowPlaying>().text,
        "Now playing: C418 - cat"
    );
    send(&mut app, 0, &|world| records(world) == 0);
}
