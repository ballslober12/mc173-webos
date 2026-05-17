# Minecraft Beta 1.7.3 Server for LG TV (ARMv7)

A work-in-progress Minecraft beta 1.7.3 server made in Rust.  
**This fork includes fixes for LG webOS TV (ARMv7, Linux kernel 3.10+).**

## Fixes in this fork

- Fixed `entity_id` starting from 1 (client compatibility)
- Fixed chunk loading order (prevents client disconnect)
- Added KeepAlive every 5 seconds
- Added delays in login handshake
- Changed bind address to `0.0.0.0` (network access)
- Added zlib decompression fallback for packet errors

## Building for LG TV (ARMv7)

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add ARM target
rustup target add armv7-unknown-linux-gnueabihf

# Install cross-compilation tools
sudo apt install gcc-arm-linux-gnueabihf
