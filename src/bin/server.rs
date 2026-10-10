//! A dedicated server for Beta 1.7.3 clients: `cargo run --bin server`.
//!
//! It links the same `game` library the client does and opens no window.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use game::networking::beta::BetaServer;
use game::networking::beta::ServerConfig;
use game::world::persistence::SaveFormat;
use game::world::persistence::WorldStorage;
use game::world::persistence::list_worlds;

const USAGE: &str = "\
Usage: server [options]

  --port <port>      Port to listen on (default 25565)
  --world <name>     World to host, by name or by folder under the saves
                     directory; created if there is none (default \"Server\")
  --saves <folder>   Where worlds are kept (default \"saves\")
  --seed <number>    Seed for a world that has to be created
  --beta-format      Create the world in Beta 1.7.3's own save format
  --view <chunks>    Chunks each way sent to a client (default 8)
  --monsters         Use the world's own difficulty instead of Peaceful.
                     Clients cannot see mobs yet.
";

struct Options {
    port: u16,
    world: String,
    saves: PathBuf,
    seed: Option<u64>,
    format: SaveFormat,
    view: i32,
    monsters: bool,
}

fn options() -> Result<Options, String> {
    let mut options = Options {
        port: 25565,
        world: "Server".to_owned(),
        saves: PathBuf::from("saves"),
        seed: None,
        format: SaveFormat::Binary,
        view: 8,
        monsters: false,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--port" => options.port = value()?.parse().map_err(|_| "bad port")?,
            "--world" => options.world = value()?,
            "--saves" => options.saves = PathBuf::from(value()?),
            "--seed" => {
                let text = value()?;
                // A number is the seed; anything else is hashed, as a word
                // typed into Beta's seed box is.
                options.seed = Some(text.parse::<i64>().map_or_else(
                    |_| {
                        text.chars()
                            .fold(0i32, |hash, c| hash.wrapping_mul(31).wrapping_add(c as i32))
                            as i64 as u64
                    },
                    |seed| seed as u64,
                ));
            }
            "--beta-format" => options.format = SaveFormat::Original,
            "--view" => options.view = value()?.parse().map_err(|_| "bad view distance")?,
            "--monsters" => options.monsters = true,
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("unknown option {other}")),
        }
    }
    options.view = options.view.clamp(2, 16);
    Ok(options)
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
    WorldStorage::create_in_format(&options.saves, seed, &options.world, None, options.format)
}

fn main() {
    let options = match options() {
        Ok(options) => options,
        Err(problem) => {
            if !problem.is_empty() {
                eprintln!("{problem}\n");
            }
            eprint!("{USAGE}");
            std::process::exit(if problem.is_empty() { 0 } else { 2 });
        }
    };
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
        peaceful: !options.monsters,
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
