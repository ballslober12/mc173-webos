//! A Minecraft beta 1.7.3 server in Rust.

use std::sync::atomic::{AtomicBool, Ordering};

use mc173::world::Dimension;

// The common configuration of the server.
pub mod config;

// The network modules, net is generic and proto is the implementation for b1.7.3.
pub mod net;
pub mod proto;

// This modules use each others, this is usually a bad design but here this was too huge
// for a single module and it will be easier to maintain like this.  
pub mod world;
pub mod chunk;
pub mod entity;
pub mod offline;
pub mod playerdata;
pub mod player;
pub mod command;
pub mod ops;
pub mod sysinfo;
pub mod console;

// This module link the previous ones to make a fully functional, multi-world server.
pub mod server;

/// Storing true while the server should run. Made `pub(crate)` so the server console's
/// `stop` command (see `console.rs` / `server.rs`) can request shutdown the same way
/// Ctrl+C does.
pub(crate) static RUNNING: AtomicBool = AtomicBool::new(true);


/// Entrypoint!
pub fn main() {

    init_tracing();
    ops::load();

    ctrlc::set_handler(|| RUNNING.store(false, Ordering::Relaxed)).unwrap();

    let mut server = server::Server::bind("0.0.0.0:25565".parse().unwrap()).unwrap();
    server.register_world(format!("overworld"), Dimension::Overworld);

    while RUNNING.load(Ordering::Relaxed) {
        server.tick_padded().unwrap();
    }

    server.stop();
    
}

/// Initialize tracing to output into the console.
fn init_tracing() {

    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::EnvFilter;

    // Default to "info": "debug" produces a lot of log traffic, which is costly when
    // stderr ends up redirected to a log file on flash/NAND storage. Set the
    // RUST_LOG env var to override this (e.g. RUST_LOG=debug) when needed.
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false);
    
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

}
