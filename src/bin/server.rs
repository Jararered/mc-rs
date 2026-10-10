//! A dedicated server for Beta 1.7.3 clients: `cargo run --bin server`.
//!
//! It links the same `game` library the client does and opens no window.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use clap::Parser;
use game::networking::beta::BetaServer;
use game::networking::beta::ServerConfig;
use game::random::parse_seed;
use game::world::persistence::SaveFormat;
use game::world::persistence::WorldStorage;
use game::world::persistence::list_worlds;

/// A dedicated server for Beta 1.7.3 clients.
#[derive(Parser)]
#[command(name = "server", version)]
struct Options {
    /// Port to listen on
    #[arg(long, default_value_t = 25565)]
    port: u16,
    /// World to host, by name or by folder under the saves directory; created
    /// if there is none
    #[arg(long, value_name = "NAME", default_value = "Server")]
    world: String,
    /// Where worlds are kept
    #[arg(long, value_name = "FOLDER", default_value = "saves")]
    saves: PathBuf,
    /// Seed for a world that has to be created: a number, or text hashed as a
    /// word typed into Beta's seed box is
    // A seed is often negative, which would otherwise read as a flag.
    #[arg(long, value_name = "SEED", allow_hyphen_values = true, value_parser = seed)]
    seed: Option<u64>,
    /// Create the world in Beta 1.7.3's own save format
    #[arg(long)]
    beta_format: bool,
    /// Chunks each way sent to a client, from 2 to 16
    #[arg(long, value_name = "CHUNKS", default_value_t = 8)]
    view: i32,
    /// Run the world on Peaceful whatever difficulty it was saved with
    #[arg(long)]
    peaceful: bool,
}

#[allow(clippy::unnecessary_wraps)]
fn seed(text: &str) -> Result<u64, Infallible> {
    Ok(parse_seed(text))
}

fn open_world(options: &Options) -> std::io::Result<WorldStorage> {
    let existing = list_worlds(&options.saves).into_iter().find(|world| {
        world.manifest.name == options.world
            || world
                .root
                .file_name()
                .is_some_and(|folder| folder == options.world.as_str())
    });
    if let Some(world) = existing {
        return WorldStorage::open(world.root);
    }
    std::fs::create_dir_all(&options.saves)?;
    let seed = options.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos() as u64)
    });
    let format = if options.beta_format {
        SaveFormat::Original
    } else {
        SaveFormat::Binary
    };
    WorldStorage::create_in_format(&options.saves, seed, &options.world, None, format)
}

fn main() {
    let mut options = Options::parse();
    options.view = options.view.clamp(2, 16);
    let storage = match open_world(&options) {
        Ok(storage) => storage,
        Err(error) => {
            eprintln!("Could not open the world \"{}\": {error}", options.world);
            std::process::exit(1);
        }
    };
    println!(
        "World \"{}\" at {} (seed {})",
        storage.manifest().name,
        storage.root().display(),
        storage.seed() as i64
    );
    let config = ServerConfig {
        view_distance: options.view,
        peaceful: options.peaceful,
        ..ServerConfig::default()
    };
    let mut server = match BetaServer::bind(("0.0.0.0", options.port), storage, config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("Could not listen on port {}: {error}", options.port);
            std::process::exit(1);
        }
    };
    println!(
        "Listening for Beta 1.7.3 clients on port {}. Press Enter to stop.",
        options.port
    );

    // A line on standard input stops the server, so the world is saved on
    // the way out without a signal handler.
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut line = String::new();
            // End of input (no terminal) is not a request to stop.
            if std::io::stdin()
                .read_line(&mut line)
                .is_ok_and(|read| read > 0)
            {
                stop.store(true, Ordering::Relaxed);
            }
        });
    }

    let step = Duration::from_millis(50);
    let mut next = Instant::now();
    let mut online = 0;
    while !stop.load(Ordering::Relaxed) {
        server.tick();
        let players = server.players();
        if players.len() != online {
            online = players.len();
            println!("{online} online: {}", players.join(", "));
        }
        next += step;
        match next.checked_duration_since(Instant::now()) {
            Some(wait) => std::thread::sleep(wait),
            // Running behind: do not try to make the lost ticks up.
            None => next = Instant::now(),
        }
    }

    println!("Saving...");
    server.close();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !server.host().loaded().is_empty() && Instant::now() < deadline {
        server.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("Stopped.");
}
