//! The sound files. They are Beta's own, from the `resources` folder its
//! launcher filled, and the player supplies them: the game runs silent
//! without the folder.
//!
//! `Minecraft.installResource` sorts the folder's top level into pools:
//! `sound/` and `newsound/` are effects, `music/` and `newmusic/` background
//! music, `streaming/` the records, some of them scrambled as `.mus`.
//! Anything else a later launcher left there (`sound3/`, icons) is not read.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use bevy::audio::AudioSource;
use bevy::audio::Decodable;
use bevy::prelude::*;
use bevy::tasks::IoTaskPool;
use bevy::tasks::Task;
use bevy::tasks::futures::check_ready;

use super::time_seed;
use crate::random::JavaRandom;

/// Beta's `.minecraft/resources`, beside `assets/`.
pub const RESOURCES_DIR: &str = "resources";

const EFFECT_FOLDERS: [&str; 2] = ["sound", "newsound"];
const MUSIC_FOLDERS: [&str; 2] = ["music", "newmusic"];
const RECORD_FOLDER: &str = "streaming";

/// `SoundPool.addSound`'s name for a file, from its path below a pool's
/// folder: what comes before the first dot, without the digits that number
/// its variants, with dots for slashes. `step/grass3.ogg` is `step.grass`.
pub fn pool_name(path: &str) -> Option<String> {
    let stem = &path[..path.find('.')?];
    let stem = stem.trim_end_matches(|digit: char| digit.is_ascii_digit());
    (!stem.is_empty()).then(|| stem.replace('/', "."))
}

/// The files under `resources/` that Beta would load.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// Pool name and file for every effect.
    pub effects: Vec<(String, PathBuf)>,
    /// Every background track. Beta picks among them all, whatever their names.
    pub music: Vec<PathBuf>,
    /// Record name and file for every disc. The streaming pool keeps a
    /// name's digits, so `13.ogg` and `13.mus` are both the record `13`.
    pub records: Vec<(String, PathBuf)>,
}

/// Lists the Ogg files in `root`'s pool folders, in name order. A folder
/// that is missing has nothing in it.
pub fn scan(root: &Path) -> Scan {
    let mut scan = Scan::default();
    let mut files = Vec::new();
    for folder in EFFECT_FOLDERS {
        collect_oggs(&root.join(folder), "", &mut files);
    }
    for (relative, path) in files.drain(..) {
        if let Some(name) = pool_name(&relative) {
            scan.effects.push((name, path));
        }
    }
    for folder in MUSIC_FOLDERS {
        collect_oggs(&root.join(folder), "", &mut files);
    }
    scan.music.extend(files.drain(..).map(|(_, path)| path));
    collect_files(&root.join(RECORD_FOLDER), "", &["ogg", "mus"], &mut files);
    for (relative, path) in files {
        if let Some(dot) = relative.find('.').filter(|dot| *dot > 0) {
            scan.records.push((relative[..dot].replace('/', "."), path));
        }
    }
    scan
}

fn collect_oggs(directory: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>) {
    collect_files(directory, prefix, &["ogg"], files);
}

fn collect_files(
    directory: &Path,
    prefix: &str,
    extensions: &[&str],
    files: &mut Vec<(String, PathBuf)>,
) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let relative = format!("{prefix}{name}");
        if path.is_dir() {
            collect_files(&path, &format!("{relative}/"), extensions, files);
        } else if path.extension().is_some_and(|extension| {
            extensions
                .iter()
                .any(|wanted| extension.eq_ignore_ascii_case(wanted))
        }) {
            files.push((relative, path));
        }
    }
}

/// `MusInputStream`: a `.mus` file is an Ogg file with each byte XORed
/// against a running hash that starts from Java's `hashCode` of the file's
/// name.
pub fn unscramble_mus(file_name: &str, bytes: &mut [u8]) {
    let mut hash = file_name.encode_utf16().fold(0i32, |hash, unit| {
        hash.wrapping_mul(31).wrapping_add(i32::from(unit))
    });
    for byte in bytes {
        *byte ^= (hash >> 8) as u8;
        hash = hash
            .wrapping_mul(498_729_871)
            .wrapping_add(85731_i32.wrapping_mul(i32::from(*byte as i8)));
    }
}

/// Reads one Ogg Vorbis file (or a `.mus` one), or `None` for one that
/// cannot be read or does not decode.
pub fn read_source(path: &Path) -> Option<AudioSource> {
    let mut bytes = std::fs::read(path).ok()?;
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mus"))
    {
        unscramble_mus(path.file_name()?.to_str()?, &mut bytes);
    }
    if !bytes.starts_with(b"OggS") {
        return None;
    }
    let source = AudioSource {
        bytes: bytes.into(),
    };
    // Bevy unwraps the decoder every time a sound starts, so a file whose
    // headers do not parse would stop the game there. Find that out here.
    std::panic::catch_unwind(|| drop(source.decoder()))
        .is_ok()
        .then_some(source)
}

/// What the loading task hands back.
pub struct Loaded {
    effects: HashMap<String, Vec<AudioSource>>,
    music: Vec<PathBuf>,
    records: HashMap<String, Vec<PathBuf>>,
    /// Files that would not decode.
    rejected: usize,
}

fn load(root: &Path) -> Loaded {
    let scan = scan(root);
    let mut effects: HashMap<String, Vec<AudioSource>> = HashMap::new();
    let mut rejected = 0;
    for (name, path) in scan.effects {
        if let Some(source) = read_source(&path) {
            effects.entry(name).or_default().push(source);
        } else {
            warn!(
                "Skipping {}: not a readable Ogg Vorbis file",
                path.display()
            );
            rejected += 1;
        }
    }
    let mut records: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for (name, path) in scan.records {
        records.entry(name).or_default().push(path);
    }
    Loaded {
        effects,
        music: scan.music,
        records,
        rejected,
    }
}

/// Every sound the game can play, by Beta's pool name.
#[derive(Resource)]
pub struct SoundLibrary {
    effects: HashMap<String, Vec<AudioSource>>,
    music: Vec<PathBuf>,
    records: HashMap<String, Vec<PathBuf>>,
    /// `SoundPool.rand`.
    random: JavaRandom,
    loading: Option<Task<Loaded>>,
}

impl Default for SoundLibrary {
    fn default() -> Self {
        Self {
            effects: HashMap::new(),
            music: Vec::new(),
            records: HashMap::new(),
            random: JavaRandom::new(time_seed()),
            loading: None,
        }
    }
}

impl SoundLibrary {
    /// `SoundPool.getRandomSoundFromSoundPool`: one of a sound's variants.
    pub fn pick(&mut self, name: &str) -> Option<AudioSource> {
        let variants = self.effects.get(name)?;
        let index = self.random.next_int(variants.len() as u32) as usize;
        Some(variants[index].clone())
    }

    /// `SoundPool.getRandomSound` over the music pool.
    pub fn pick_music(&mut self) -> Option<PathBuf> {
        if self.music.is_empty() {
            return None;
        }
        let index = self.random.next_int(self.music.len() as u32) as usize;
        Some(self.music[index].clone())
    }

    /// `getRandomSoundFromSoundPool` over the streaming pool: one of the
    /// files of the record `name`.
    pub fn pick_record(&mut self, name: &str) -> Option<PathBuf> {
        let files = self.records.get(name)?;
        let index = self.random.next_int(files.len() as u32) as usize;
        Some(files[index].clone())
    }

    pub fn has(&self, name: &str) -> bool {
        self.effects.contains_key(name)
    }

    pub fn random(&mut self) -> &mut JavaRandom {
        &mut self.random
    }
}

/// Reads `resources/` on the IO pool, so no frame waits on the disk.
pub(super) fn start_loading(mut library: ResMut<SoundLibrary>) {
    library.loading = Some(IoTaskPool::get().spawn(async { load(Path::new(RESOURCES_DIR)) }));
}

pub(super) fn finish_loading(mut library: ResMut<SoundLibrary>) {
    let Some(loaded) = library.loading.as_mut().and_then(check_ready) else {
        return;
    };
    library.loading = None;
    let files: usize = loaded.effects.values().map(Vec::len).sum();
    if files == 0 && loaded.music.is_empty() {
        info!("No sounds in {RESOURCES_DIR}/: the game is silent");
    } else {
        info!(
            "Loaded {files} sounds in {} pools, {} music tracks and {} records from {RESOURCES_DIR}/",
            loaded.effects.len(),
            loaded.music.len(),
            loaded.records.len(),
        );
    }
    if loaded.rejected > 0 {
        warn!("{} sound files could not be decoded", loaded.rejected);
    }
    library.effects = loaded.effects;
    library.music = loaded.music;
    library.records = loaded.records;
}

pub(super) fn is_loading(library: Res<SoundLibrary>) -> bool {
    library.loading.is_some()
}
