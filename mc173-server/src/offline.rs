//! Offline player data.

use glam::{DVec3, Vec2};

use mc173::item::ItemStack;


/// Every time a player position is saved (autosave, disconnect, shutdown), its Y is
/// raised by this many blocks, so on the next login the player is placed above where
/// they were instead of possibly inside a block.
pub const SAVE_Y_OFFSET: f64 = 2.0;

/// An offline player defines the saved data of a player that is not connected.
#[derive(Debug, Clone)]
pub struct OfflinePlayer {
    /// World name.
    pub world: String,
    /// Last saved position of the player.
    pub pos: DVec3,
    /// Last saved look of the player.
    pub look: Vec2,
    /// Last saved main inventory (36 slots, hotbar included in the first 9).
    pub main_inv: Box<[ItemStack; 36]>,
    /// Last saved armor inventory (4 slots).
    pub armor_inv: Box<[ItemStack; 4]>,
    /// Last saved selected hotbar slot (0..9).
    pub hand_slot: u8,
    /// The player's home, set with `/sethome`: world name, position and look.
    pub home: Option<(String, DVec3, Vec2)>,
}

impl OfflinePlayer {

    /// Construct a fresh offline player (empty inventory, no home) at the given
    /// world/position/look, used the first time a username is ever seen.
    pub fn new(world: String, pos: DVec3, look: Vec2) -> Self {
        Self {
            world,
            pos,
            look,
            main_inv: Box::new([ItemStack::EMPTY; 36]),
            armor_inv: Box::new([ItemStack::EMPTY; 4]),
            hand_slot: 0,
            home: None,
        }
    }

}
