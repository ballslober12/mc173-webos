//! The network server managing connected players and dispatching incoming packets.

use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::io;

use glam::Vec2;

use tracing::{warn, info};

use mc173::world::{Dimension, Weather};
use mc173::entity::{self as e};

use crate::config;
use crate::proto::{self, Network, NetworkEvent, NetworkClient, InPacket, OutPacket};
use crate::offline::OfflinePlayer;
use crate::player::ServerPlayer;
use crate::world::ServerWorld;
use crate::playerdata;
use crate::console::Console;
use crate::sysinfo;
use crate::ops;

const TICK_DURATION: Duration = Duration::from_millis(50);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
/// How often connected players are autosaved to disk (position, look, inventory,
/// home). This is a trade-off between data safety (surviving a crash/power loss) and
/// NAND wear (each autosave is a small write per online player): once a minute keeps
/// both writes and potential data loss small.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);

pub struct Server {
    net: Network,
    clients: HashMap<NetworkClient, ClientState>,
    worlds: Vec<WorldState>,
    offline_players: HashMap<String, OfflinePlayer>,
    last_keepalive: Instant,
    last_autosave: Instant,
    /// Reads commands typed into the server's own terminal, see `tick_console`.
    console: Console,
}

impl Server {
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        info!("server bound to {addr}");
        info!("console ready, type 'help' for a list of commands");
        Ok(Self {
            net: Network::bind(addr)?,
            clients: HashMap::new(),
            worlds: vec![],
            offline_players: HashMap::new(),
            last_keepalive: Instant::now(),
            last_autosave: Instant::now(),
            console: Console::spawn(),
        })
    }

    pub fn register_world(&mut self, name: String, dimension: Dimension) {
        self.worlds.push(WorldState {
            world: ServerWorld::new(name, dimension),
            players: Vec::new(),
        });
    }

    /// Save every connected player's data and every world's chunks, blocking until all
    /// of it has been flushed to disk. Called on graceful shutdown.
    pub fn stop(&mut self) {
        for state in &mut self.worlds {

            for player in &state.players {
                let offline = player.to_offline(&state.world.name);
                if let Err(err) = playerdata::save(&player.username, &offline) {
                    warn!("failed to save player data for {}: {err}", player.username);
                }
            }

            state.world.stop();
        }
    }

    pub fn tick_padded(&mut self) -> io::Result<()> {
        let start = Instant::now();
        self.tick()?;
        let elapsed = start.elapsed();
        if let Some(missing) = TICK_DURATION.checked_sub(elapsed) {
            std::thread::sleep(missing);
        } else {
            warn!("tick too long {:?}, expected {:?}", elapsed, TICK_DURATION);
        }
        Ok(())
    }

    pub fn tick(&mut self) -> io::Result<()> {
        self.tick_console();
        self.tick_net()?;
        self.tick_autosave();
        for state in &mut self.worlds {
            state.world.tick(&mut state.players);
        }
        Ok(())
    }

    /// Drain and run every console command line queued since the last tick (see
    /// `console.rs`). Console commands are a small, separate set from in-game player
    /// commands (`command.rs`): most player commands need a player entity (position,
    /// inventory, ...) which the console does not have, so rather than force-fitting
    /// the console into that system, it gets its own minimal handler here, sharing
    /// the `ops` and `sysinfo` modules where it makes sense.
    fn tick_console(&mut self) {
        while let Some(line) = self.console.poll() {
            self.handle_console_command(&line);
        }
    }

    fn handle_console_command(&mut self, line: &str) {

        let line = line.trim();
        if line.is_empty() {
            return;
        }

        // Accept both "op Steve" and "/op Steve": typing the leading slash out of
        // habit (from in-game chat) should just work.
        let line = line.strip_prefix('/').unwrap_or(line);
        let parts = line.split_whitespace().collect::<Vec<_>>();
        let Some(&cmd) = parts.first() else { return };
        let args = &parts[1..];

        match cmd {
            "help" => {
                println!("Available console commands:");
                println!("  help                 Show this message");
                println!("  info                 Show system and server info");
                println!("  perf                 Show per-world performance indicators");
                println!("  list                 List connected players");
                println!("  op <player>          Give a player operator rights");
                println!("  deop <player>        Remove a player's operator rights");
                println!("  ops                  List all operators");
                println!("  say <message>        Broadcast a message to every connected player");
                println!("  stop                 Save everything and stop the server");
            }
            "info" => self.console_info(),
            "perf" => self.console_perf(),
            "list" => self.console_list(),
            "op" => match args {
                [username] => {
                    if !ops::is_valid_username(username) {
                        println!("invalid username: {username}");
                    } else if ops::add(username) {
                        println!("gave operator rights to: {username}");
                    } else {
                        println!("{username} is already an operator");
                    }
                }
                _ => println!("usage: op <player>"),
            }
            "deop" => match args {
                [username] => {
                    if ops::remove(username) {
                        println!("removed operator rights from: {username}");
                    } else {
                        println!("{username} is not an operator");
                    }
                }
                _ => println!("usage: deop <player>"),
            }
            "ops" => {
                let names = ops::list();
                if names.is_empty() {
                    println!("operators: none");
                } else {
                    println!("operators: {}", names.join(", "));
                }
            }
            "say" => {
                if args.is_empty() {
                    println!("usage: say <message>");
                } else {
                    let message = format!("[Server] {}", args.join(" "));
                    info!("{message}");
                    for state in &self.worlds {
                        for player in &state.players {
                            player.send_chat(message.clone());
                        }
                    }
                }
            }
            "stop" => {
                println!("stopping...");
                crate::RUNNING.store(false, std::sync::atomic::Ordering::Relaxed);
            }
            _ => println!("unknown command: {cmd} (type 'help')"),
        }

    }

    fn console_info(&self) {

        println!("uname -a: {}", sysinfo::uname());

        match sysinfo::meminfo() {
            Some((total_kb, available_kb)) => {
                let used_kb = total_kb.saturating_sub(available_kb);
                println!("memory: {:.1} MB used / {:.1} MB free / {:.1} MB total",
                    used_kb as f64 / 1024.0, available_kb as f64 / 1024.0, total_kb as f64 / 1024.0);
            }
            None => println!("memory: unavailable (could not read /proc/meminfo)"),
        }

        let total_players: usize = self.worlds.iter().map(|state| state.players.len()).sum();
        println!("players online: {total_players}");

        for state in &self.worlds {
            println!("world {}: tick duration {:.1} ms", state.world.name, state.world.tick_duration.get() * 1000.0);
        }

    }

    fn console_perf(&self) {
        for state in &self.worlds {
            println!("world {}:", state.world.name);
            println!("  tick duration: {:.1} ms", state.world.tick_duration.get() * 1000.0);
            println!("  tick interval: {:.1} ms", state.world.tick_interval.get() * 1000.0);
            println!("  events: {:.1}", state.world.events_count.get());
            println!("  entities: {} ({} players)",
                state.world.world.get_entity_count(), state.world.world.get_player_entity_count());
            println!("  block ticks: {}", state.world.world.get_block_tick_count());
            println!("  light updates: {}", state.world.world.get_light_update_count());
        }
    }

    fn console_list(&self) {
        let mut count = 0;
        for state in &self.worlds {
            for player in &state.players {
                println!("  {} ({})", player.username, state.world.name);
                count += 1;
            }
        }
        println!("{count} player(s) online");
    }

    fn tick_net(&mut self) -> io::Result<()> {
        // Send KeepAlive every 5 seconds
        if self.last_keepalive.elapsed() > KEEPALIVE_INTERVAL {
            for client in self.clients.keys() {
                self.net.send(*client, OutPacket::KeepAlive);
            }
            self.last_keepalive = Instant::now();
        }
        
        while let Some(event) = self.net.poll()? {
            match event {
                NetworkEvent::Accept { client } => self.handle_accept(client),
                NetworkEvent::Lost { client, error } => self.handle_lost(client, error),
                NetworkEvent::Packet { client, packet } => self.handle_packet(client, packet),
            }
        }
        Ok(())
    }

    /// Periodically persist every connected player's data to disk, so that a crash or
    /// power loss (a real risk on the kind of embedded device this targets) loses at
    /// most a minute of progress instead of everything since the last clean shutdown.
    fn tick_autosave(&mut self) {
        if self.last_autosave.elapsed() < AUTOSAVE_INTERVAL {
            return;
        }
        self.last_autosave = Instant::now();

        for state in &self.worlds {
            for player in &state.players {
                let offline = player.to_offline(&state.world.name);
                if let Err(err) = playerdata::save(&player.username, &offline) {
                    warn!("failed to autosave player data for {}: {err}", player.username);
                }
            }
        }
    }

    fn handle_accept(&mut self, client: NetworkClient) {
        info!("accept client #{}", client.id());
        self.clients.insert(client, ClientState::Handshaking);
    }

    fn handle_lost(&mut self, client: NetworkClient, error: Option<io::Error>) {
        info!("lost client #{}: {:?}", client.id(), error);
        let state = self.clients.remove(&client).unwrap();
        if let ClientState::Playing { world_index, player_index } = state {
            let state = &mut self.worlds[world_index];
            let mut player = state.players.swap_remove(player_index);

            // Persist the player's position, look, inventory and home before the
            // entity is removed, so progress survives disconnects and crashes, not
            // just clean shutdowns.
            let offline = player.to_offline(&state.world.name);
            if let Err(err) = playerdata::save(&player.username, &offline) {
                warn!("failed to save player data for {}: {err}", player.username);
            }
            self.offline_players.insert(player.username.clone(), offline);

            state.world.handle_player_leave(&mut player, true);
            if let Some(swapped_player) = state.players.get(player_index) {
                self.clients.insert(swapped_player.client, ClientState::Playing {
                    world_index,
                    player_index,
                }).expect("swapped player should have a previous state");
            }
        }
    }

    fn handle_packet(&mut self, client: NetworkClient, packet: InPacket) {
        match *self.clients.get(&client).unwrap() {
            ClientState::Handshaking => self.handle_handshaking(client, packet),
            ClientState::Playing { world_index, player_index } => {
                let state = &mut self.worlds[world_index];
                let player = &mut state.players[player_index];
                player.handle(&mut state.world, packet);
            }
        }
    }

    fn handle_handshaking(&mut self, client: NetworkClient, packet: InPacket) {
        match packet {
            InPacket::KeepAlive => {}
            InPacket::Handshake(_) => {
                self.handle_handshake(client);
            }
            InPacket::Login(packet) => {
                self.handle_login(client, packet);
            }
            _ => {
                self.send_disconnect(client, format!("Invalid packet"));
            }
        }
    }

    fn handle_handshake(&mut self, client: NetworkClient) {
        self.net.send(client, OutPacket::Handshake(proto::OutHandshakePacket {
            server: "-".to_string(),
        }));
    }

    fn handle_login(&mut self, client: NetworkClient, packet: proto::InLoginPacket) {

        if packet.protocol_version != 14 {
            self.send_disconnect(client, format!("Protocol version mismatch!"));
            return;
        }

        let spawn_pos = config::SPAWN_POS;

        let offline_player = self.offline_players.entry(packet.username.clone())
            .or_insert_with(|| {
                // Try to load previously saved data (position, look, inventory, home)
                // from disk first, and only fall back to a fresh spawn if there is
                // none yet (or it could not be read).
                playerdata::load(&packet.username).unwrap_or_else(|| {
                    let state = &self.worlds[0];
                    OfflinePlayer::new(state.world.name.clone(), spawn_pos, Vec2::ZERO)
                })
            });

        let (world_index, state) = self.worlds.iter_mut()
            .enumerate()
            .filter(|(_, state)| state.world.name == offline_player.world)
            .next()
            .expect("invalid offline player world name");

        let entity = e::Human::new_with(|base, living, player| {
            base.pos = offline_player.pos;
            base.look = offline_player.look;
            base.persistent = false;
            base.can_pickup = true;
            living.artificial = true;
            living.health = 200;
            player.username = packet.username.clone();
        });

        let entity_id = state.world.world.spawn_entity(entity);
        state.world.world.set_player_entity(entity_id, true);

        // Login packet
        self.net.send(client, OutPacket::Login(proto::OutLoginPacket {
            entity_id,
            random_seed: state.world.seed,
            dimension: match state.world.world.get_dimension() {
                Dimension::Overworld => 0,
                Dimension::Nether => -1,
            },
        }));

        // Spawn position
        self.net.send(client, OutPacket::SpawnPosition(proto::SpawnPositionPacket {
            pos: spawn_pos.as_ivec3(),
        }));

        // Position look
        self.net.send(client, OutPacket::PositionLook(proto::PositionLookPacket {
            pos: offline_player.pos,
            stance: offline_player.pos.y + 1.62,
            look: offline_player.look,
            on_ground: false,
        }));

        // Update time
        self.net.send(client, OutPacket::UpdateTime(proto::UpdateTimePacket {
            time: state.world.world.get_time(),
        }));

        let mut player = ServerPlayer::new(&self.net, client, entity_id, packet.username, &offline_player);
        state.world.handle_player_join(&mut player);
        let player_index = state.players.len();
        state.players.push(player);

        // Force an initial chunk update so the player doesn't wait for a movement
        // packet before anything around them loads.
        if let Some(player) = state.players.last_mut() {
            player.update_chunks(&state.world);
        }

        // Resend PositionLook to confirm
        self.net.send(client, OutPacket::PositionLook(proto::PositionLookPacket {
            pos: offline_player.pos,
            stance: offline_player.pos.y + 1.62,
            look: offline_player.look,
            on_ground: false,
        }));

        let previous_state = self.clients.insert(client, ClientState::Playing {
            world_index,
            player_index,
        });

        debug_assert_eq!(previous_state, Some(ClientState::Handshaking));
    }

    fn send_disconnect(&mut self, client: NetworkClient, reason: String) {
        self.net.send(client, OutPacket::Disconnect(proto::DisconnectPacket {
            reason,
        }));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientState {
    Handshaking,
    Playing {
        world_index: usize,
        player_index: usize,
    }
}

struct WorldState {
    world: ServerWorld,
    players: Vec<ServerPlayer>,
}
