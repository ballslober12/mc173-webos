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

const TICK_DURATION: Duration = Duration::from_millis(50);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);

pub struct Server {
    net: Network,
    clients: HashMap<NetworkClient, ClientState>,
    worlds: Vec<WorldState>,
    offline_players: HashMap<String, OfflinePlayer>,
    last_keepalive: Instant,
}

impl Server {
    pub fn bind(addr: SocketAddr) -> io::Result<Self> {
        info!("server bound to {addr}");
        Ok(Self {
            net: Network::bind(addr)?,
            clients: HashMap::new(),
            worlds: vec![],
            offline_players: HashMap::new(),
            last_keepalive: Instant::now(),
        })
    }

    pub fn register_world(&mut self, name: String, dimension: Dimension) {
        self.worlds.push(WorldState {
            world: ServerWorld::new(name, dimension),
            players: Vec::new(),
        });
    }

    pub fn stop(&mut self) {
        for state in &mut self.worlds {
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
        self.tick_net()?;
        for state in &mut self.worlds {
            state.world.tick(&mut state.players);
        }
        Ok(())
    }

    fn tick_net(&mut self) -> io::Result<()> {
        // Send KeepAlive every 5 seconds
        if self.last_keepalive.elapsed() > KEEPALIVE_INTERVAL {
            for client in self.clients.keys() {
                self.net.send(*client, OutPacket::KeepAlive);
                eprintln!("[DEBUG] Sent KeepAlive to client #{}", client.id());
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
        eprintln!("[DEBUG] handle_packet called");
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
        eprintln!("[DEBUG] handle_handshaking called");
        match packet {
            InPacket::KeepAlive => eprintln!("[DEBUG] KeepAlive packet"),
            InPacket::Handshake(_) => {
                eprintln!("[DEBUG] Handshake packet -> calling handle_handshake");
                self.handle_handshake(client);
            }
            InPacket::Login(packet) => {
                eprintln!("[DEBUG] Login packet -> calling handle_login, protocol: {}", packet.protocol_version);
                self.handle_login(client, packet);
            }
            _ => {
                eprintln!("[DEBUG] Unexpected packet, disconnecting");
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
        eprintln!("[DEBUG] handle_login called, username: {}", packet.username);
        eprintln!("[DEBUG] protocol_version: {}, expecting 14", packet.protocol_version);
        
        if packet.protocol_version != 14 {
            eprintln!("[DEBUG] Protocol mismatch!");
            self.send_disconnect(client, format!("Protocol version mismatch!"));
            return;
        }
        eprintln!("[DEBUG] Protocol OK, proceeding...");
        
        let spawn_pos = config::SPAWN_POS;
        eprintln!("[DEBUG] spawn_pos: {:?}", spawn_pos);

        let offline_player = self.offline_players.entry(packet.username.clone())
            .or_insert_with(|| {
                let state = &self.worlds[0];
                OfflinePlayer {
                    world: state.world.name.clone(),
                    pos: spawn_pos,
                    look: Vec2::ZERO,
                }
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
        eprintln!("[DEBUG] Spawned entity with id: {}", entity_id);

        // Login packet
        self.net.send(client, OutPacket::Login(proto::OutLoginPacket {
            entity_id,
            random_seed: state.world.seed,
            dimension: match state.world.world.get_dimension() {
                Dimension::Overworld => 0,
                Dimension::Nether => -1,
            },
        }));
        eprintln!("[DEBUG] Sent Login packet");

        // Spawn position
        self.net.send(client, OutPacket::SpawnPosition(proto::SpawnPositionPacket {
            pos: spawn_pos.as_ivec3(),
        }));
        eprintln!("[DEBUG] Sent SpawnPosition packet");

        // Position look
        self.net.send(client, OutPacket::PositionLook(proto::PositionLookPacket {
            pos: offline_player.pos,
            stance: offline_player.pos.y + 1.62,
            look: offline_player.look,
            on_ground: false,
        }));
        eprintln!("[DEBUG] Sent PositionLook packet");

        // Update time
        self.net.send(client, OutPacket::UpdateTime(proto::UpdateTimePacket {
            time: state.world.world.get_time(),
        }));
        eprintln!("[DEBUG] Sent UpdateTime packet");

let mut player = ServerPlayer::new(&self.net, client, entity_id, packet.username, &offline_player);
state.world.handle_player_join(&mut player);
let player_index = state.players.len();
state.players.push(player);

// Получаем ссылку на игрока и форсируем обновление чанков
if let Some(player) = state.players.last_mut() {
    player.update_chunks(&state.world);
    eprintln!("[DEBUG] Forced chunk update for player");
}
        // Resend PositionLook to confirm
        self.net.send(client, OutPacket::PositionLook(proto::PositionLookPacket {
            pos: offline_player.pos,
            stance: offline_player.pos.y + 1.62,
            look: offline_player.look,
            on_ground: false,
        }));
        eprintln!("[DEBUG] Resent PositionLook to confirm");

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