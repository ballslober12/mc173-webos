//! A minimal interactive console reading commands from the server's own stdin, so the
//! operator can run commands (see the `help` console command) directly in the terminal
//! the server was launched from, without needing a game client connected.
//!
//! Reading stdin blocks, so it happens on a dedicated background thread; lines are
//! queued on a channel and drained once per tick on the main thread (see
//! `Server::tick_console` in `server.rs`), since the rest of the server state is not
//! thread-safe and must only ever be touched from the tick loop.

use std::io::{self, BufRead};
use std::thread;

use crossbeam_channel::{Receiver, unbounded};

/// Handle to the background stdin-reading thread.
pub struct Console {
    rx: Receiver<String>,
}

impl Console {
    /// Start reading stdin on a dedicated thread. If stdin is not a usable input (e.g.
    /// the server is running as a background service with no attached terminal, or
    /// stdin was redirected from `/dev/null`), reading just hits EOF immediately, the
    /// thread exits, and the console is silently inert: `poll` will simply never
    /// return anything, which is the correct behavior rather than a crash.
    pub fn spawn() -> Self {

        let (tx, rx) = unbounded();

        thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        // The other end (the server) is gone, e.g. mid-shutdown:
                        // nothing more to do here.
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    // A read error on stdin (not just EOF, which yields no more
                    // items) - stop rather than spin.
                    Err(_) => break,
                }
            }
        });

        Self { rx }

    }

    /// Return the next queued console line, if any, without blocking.
    pub fn poll(&self) -> Option<String> {
        self.rx.try_recv().ok()
    }
}
