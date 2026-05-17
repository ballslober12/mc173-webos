# Minecraft Beta 1.7.3 Server for LG TV (ARMv7)

A work-in-progress Minecraft beta 1.7.3 server made in Rust.  
**This fork includes fixes for LG webOS TV (ARMv7, Linux kernel 3.10+).**

##  What Works (Tested)

- Players can connect and join the world
- Chat messages
- Block breaking/placing
- Movement and position sync
- World saving/loading
- Basic inventory
- Crafting table
- Chests, furnaces, dispensers
- Day/night cycle
- Weather (rain/thunder)
- Entity tracking (mobs, items)
- Item drops and pickup
- Commands: `/help`, `/give`, `/spawn`, `/time`, `/weather`, `/pos`, `/tick`, `/clean`, `/explode`

##  What Doesn't Work / Not Implemented

- Redstone (partial – basic circuits may work, complex won't)
- Rails (not implemented)
- Some block interactions (pistons partially work)
- Mob AI and natural spawning (entities can be spawned via command only)
- Nether dimension (Overworld only)
- Player skins (offline mode only)
- Some crafting recipes may be missing
- World serialization for player entities (players respawn at spawn after restart)

## Fixes in this fork (for LG TV compatibility)

- Fixed `entity_id` starting from 1 (required for Beta 1.7.3 client)
- Fixed chunk loading order (prevents client disconnect on slow networks)
- Added KeepAlive every 5 seconds (prevents timeout)
- Added delays in login handshake (100ms between packets)
- Changed bind address to `0.0.0.0` (allows network connections)
- Added zlib decompression fallback for packet errors
- Optimized chunk sending (one chunk per tick)

## Building for LG TV (ARMv7)

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add ARM target
rustup target add armv7-unknown-linux-gnueabihf

# Install cross-compilation tools
sudo apt install gcc-arm-linux-gnueabihf
