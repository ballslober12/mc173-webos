//! Persistence of per-player data (position, look, inventory, home) to disk, so that
//! progress is not lost when a player disconnects, the connection is lost, or the
//! server process restarts or crashes.
//!
//! Each player gets one small NBT file under `test_world/players/<username>.dat`.
//! Saves are written to a temporary file and then atomically renamed into place: on
//! flash/NAND storage a write can be interrupted by a crash or power loss, and this
//! way an interrupted save can never leave a half-written, corrupted file behind, it
//! just leaves the previous (still valid) save in place. Each save is also a single
//! whole-file write with no read-modify-write step, keeping the number of flash
//! operations per save to a minimum.

use std::path::{Path, PathBuf};
use std::io;
use std::fs;

use glam::{DVec3, Vec2};

use mc173::serde::nbt::{self, Nbt, NbtCompound};
use mc173::item::ItemStack;

use crate::offline::OfflinePlayer;

/// Directory (relative to the working directory) where player data files live.
const PLAYERS_DIR: &str = "test_world/players/";

/// Load a previously saved player from disk, if any. Any error (missing file,
/// truncated or corrupted data) is treated as "no saved data", the caller should fall
/// back to spawning a fresh player, so a corrupted file can never prevent login.
pub fn load(username: &str) -> Option<OfflinePlayer> {
    let data = fs::read(player_path(username)).ok()?;
    let root = nbt::from_reader(&data[..]).ok()?;
    from_nbt(root.as_compound()?)
}

/// Save a player's offline data to disk, atomically.
///
/// Note: the Y offset (see [`crate::offline::SAVE_Y_OFFSET`]) is NOT applied here, it is
/// applied once when the player state is snapshotted (`ServerPlayer::to_offline`), so
/// that the copy kept in memory and the copy written to disk always agree.
pub fn save(username: &str, offline: &OfflinePlayer) -> io::Result<()> {

    let dir = Path::new(PLAYERS_DIR);
    fs::create_dir_all(dir)?;

    let mut buf = Vec::with_capacity(512);
    nbt::to_writer(&mut buf, &Nbt::Compound(to_nbt(offline)))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    // Write-then-rename: `rename` on the same filesystem is atomic, so readers (the
    // next server start) only ever see either the old file or the fully-written new
    // one, never a partial one.
    let tmp_path = dir.join(format!("{username}.dat.tmp"));
    fs::write(&tmp_path, &buf)?;
    fs::rename(&tmp_path, player_path(username))?;

    Ok(())

}

fn player_path(username: &str) -> PathBuf {
    Path::new(PLAYERS_DIR).join(format!("{username}.dat"))
}

fn to_nbt(offline: &OfflinePlayer) -> NbtCompound {

    let mut comp = NbtCompound::new();

    comp.insert("World", offline.world.clone());
    comp.insert("PosX", offline.pos.x);
    comp.insert("PosY", offline.pos.y);
    comp.insert("PosZ", offline.pos.z);
    comp.insert("LookX", offline.look.x);
    comp.insert("LookY", offline.look.y);

    comp.insert("Inventory", stacks_to_nbt(&offline.main_inv[..]));
    comp.insert("Armor", stacks_to_nbt(&offline.armor_inv[..]));
    comp.insert("SelectedSlot", offline.hand_slot as i8);

    if let Some((home_world, home_pos, home_look)) = &offline.home {
        let mut home = NbtCompound::new();
        home.insert("World", home_world.clone());
        home.insert("PosX", home_pos.x);
        home.insert("PosY", home_pos.y);
        home.insert("PosZ", home_pos.z);
        home.insert("LookX", home_look.x);
        home.insert("LookY", home_look.y);
        comp.insert("Home", Nbt::Compound(home));
    }

    comp

}

fn from_nbt(comp: &NbtCompound) -> Option<OfflinePlayer> {

    let world = comp.get_string("World")?.to_string();
    let pos = DVec3::new(comp.get_double("PosX")?, comp.get_double("PosY")?, comp.get_double("PosZ")?);
    let look = Vec2::new(comp.get_float("LookX")?, comp.get_float("LookY")?);

    let mut offline = OfflinePlayer::new(world, pos, look);

    if let Some(list) = comp.get_list("Inventory") {
        nbt_to_stacks(list, &mut offline.main_inv[..]);
    }

    if let Some(list) = comp.get_list("Armor") {
        nbt_to_stacks(list, &mut offline.armor_inv[..]);
    }

    if let Some(slot) = comp.get_byte("SelectedSlot") {
        // Clamp defensively: a corrupted or foreign value should never let an
        // out-of-range hotbar slot back into the game.
        offline.hand_slot = (slot.max(0) as u8).min(8);
    }

    if let Some(home) = comp.get_compound("Home") {
        if let (Some(home_world), Some(x), Some(y), Some(z), Some(lx), Some(ly)) = (
            home.get_string("World"),
            home.get_double("PosX"), home.get_double("PosY"), home.get_double("PosZ"),
            home.get_float("LookX"), home.get_float("LookY"),
        ) {
            offline.home = Some((home_world.to_string(), DVec3::new(x, y, z), Vec2::new(lx, ly)));
        }
    }

    Some(offline)

}

/// Encode non-empty item stacks as a list of compounds, each remembering its slot
/// index, so that gaps (empty slots) do not need to be stored at all.
fn stacks_to_nbt(stacks: &[ItemStack]) -> Vec<Nbt> {
    stacks.iter().enumerate()
        .filter(|(_, stack)| !stack.is_empty())
        .map(|(index, stack)| {
            let mut comp = NbtCompound::new();
            comp.insert("Slot", index as i16);
            comp.insert("id", stack.id);
            comp.insert("Count", stack.size.min(i8::MAX as u16) as i8);
            comp.insert("Damage", stack.damage);
            Nbt::Compound(comp)
        })
        .collect()
}

fn nbt_to_stacks(list: &[Nbt], out: &mut [ItemStack]) {
    for entry in list {
        let Some(comp) = entry.as_compound() else { continue };
        let (Some(slot), Some(id), Some(count), Some(damage)) = (
            comp.get_short("Slot"), comp.get_short("id"),
            comp.get_byte("Count"), comp.get_short("Damage"),
        ) else { continue };

        let slot = slot as usize;
        if slot >= out.len() {
            continue;
        }

        out[slot] = ItemStack {
            id: id as u16,
            size: count.max(0) as u16,
            damage: damage as u16,
        };
    }
}
