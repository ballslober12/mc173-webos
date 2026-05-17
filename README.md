# mc173-webos -- Minecraft Beta 1.7.3 Server for LG TV (ARMv7)

A Minecraft Beta 1.7.3 server written in **Rust**, patched and tested for LG webOS TV (ARMv7, Linux kernel 3.10+).

This is a fork of [theorzr/mc173](https://github.com/theorzr/mc173) with critical fixes for embedded ARM devices.

**IMPORTANT: This project is written in RUST. You need Rust compiler to build it.**


## Table of Contents

1. [What Works / What Doesn't](#what-works--what-doesnt)
2. [Fixes in this Fork](#fixes-in-this-fork)
3. [Building for LG TV (RUST) - FULL STEP BY STEP](#building-for-lg-tv-rust---full-step-by-step)
4. [Running on TV](#running-on-tv)
5. [Connecting from PC](#connecting-from-pc)
6. [Commands](#commands)
7. [Troubleshooting](#troubleshooting)
8. [Original Project](#original-project)


## What Works / What Doesn't

| Feature | Status |
|---------|--------|
| Player connection, chat, movement | Works |
| Block breaking / placing | Works |
| World saving / loading | Works |
| Inventory (basic) | Works |
| Crafting table | Works |
| Chests, furnaces, dispensers | Works |
| Day/night cycle | Works |
| Weather (rain/thunder) | Works |
| Item drops and pickup | Works |
| Commands (`/help`, `/give`, etc.) | Works |
| Entity tracking | Works |
| Mob AI / natural spawning | Not implemented |
| Redstone (full) | Partial only |
| Rails | Not implemented |
| Nether dimension | Overworld only |
| Player skins | Offline mode only |


## Fixes in this Fork (for LG TV)

| Problem | Solution |
|---------|----------|
| `entity_id` started from 0 (client rejected) | Changed to start from 1 |
| Client disconnected after login | Added delays between login packets |
| Client timed out | Added KeepAlive every 5 seconds |
| Server only bound to `127.0.0.1` | Changed to `0.0.0.0` for network access |
| "Bad compressed data format" error | Added zlib fallback decompression |
| Too many chunks sent at once | Changed to 1 chunk per tick |
| Binary not found on TV | Added static build instructions |


## Building for LG TV (RUST) - FULL STEP BY STEP

### Step 1: Install Rust on your PC (WSL or Linux)

Open a terminal and run the official Rust installation script. When prompted, select option 1: "Proceed with installation (default)". After installation, reload your shell.

Verify Rust is installed by checking versions of rustc and cargo.

### Step 2: Install ARM cross-compilation target

Use rustup to add the target: armv7-unknown-linux-gnueabihf. Verify the target is installed by listing installed targets.

### Step 3: Install GCC cross-compiler for ARM

On Ubuntu/Debian, install gcc-arm-linux-gnueabihf via apt. Verify with version check.

### Step 4: Clone this repository

Clone the repository using git clone and enter the directory.

### Step 5: Configure Cargo for cross-compilation

Create a .cargo/config.toml file in the project root with the linker set to arm-linux-gnueabihf-gcc.

### Step 6: Build the server (DYNAMIC build)

Run cargo build with --release and --target armv7-unknown-linux-gnueabihf. Build takes 2-5 minutes. Binary size is approximately 4-5 MB.

### Step 7: Build the server (STATIC build - RECOMMENDED for LG TV)

Add the musl target: armv7-unknown-linux-musleabihf. Then build with cargo. Static binary size is approximately 10-15 MB but runs on any ARM Linux without external libraries.

### Step 8: Locate the compiled binary

The dynamic binary is located at target/armv7-unknown-linux-gnueabihf/release/mc173-server. The static binary is at target/armv7-unknown-linux-musleabihf/release/mc173-server.

### Step 9: Check binary architecture (important!)

Use the file command on the binary. Expected output must contain: ELF 32-bit LSB executable, ARM. If it shows x86-64, the build target is wrong — check Step 2 and Step 5.

### Step 10: Copy binary to USB drive

Copy the binary from the target folder to your USB drive (common mount points: /mnt/e/, /mnt/d/, /mnt/f/, /media/username/).


## Running on TV

### Step 1: Copy binary from USB to TV

Over SSH, navigate to /home/root, create a minecraft_server directory, and copy the binary from the USB path. Common USB paths on LG TV: /media/internal/, /media/usb0/, /tmp/usb/.

### Step 2: Make binary executable

Use chmod +x on the binary.

### Step 3: Run the server

Execute the binary with ./mc173-server.

### Step 4: Expected output

If successful, you will see a log line: server bound to 0.0.0.0:25565 followed by chunk loading messages.

### Step 5: If "not found" error

If you get "-sh: ./mc173-server: not found", try running with the dynamic linker directly: /lib/ld-linux.so.3 ./mc173-server. Alternatively, create a symlink to /lib/ld-linux.so.3.

### Step 6: Keep server running in background

Use nohup to run the server in the background and redirect output to a log file. View logs with tail. Stop the server with pkill mc173-server.


## Connecting from PC

### Step 1: Find TV IP address

On the TV via SSH, use ip addr show and look for an address like 192.168.1.100.

Launch Minecraft Beta 1.7.3 on your PC, click Multiplayer, then Direct Connect, and enter the TV's IP address followed by port 25565 (e.g., 192.168.1.100:25565).


## Commands

Supported in-game commands (type in chat with / prefix):

- /help - Show available commands
- /give <player> <item> [amount] - Give an item
- /gamemode <0/1> [player] - Change game mode
- /time <set/add> <value> - Change time of day
- /stop - Stop the server
- /list - List online players


## Troubleshooting

**Q: Binary says "not found" even though it exists**  
A: Missing dynamic libraries. Use the static build (Step 7) or run with /lib/ld-linux.so.3.

**Q: Client connects but immediately disconnects**  
A: Check that entity_id fix is present in your build (it's in this fork). Also ensure keepalive packets are being sent.

**Q: "Address already in use"**  
A: Another process is using port 25565. Kill it with pkill mc173-server or change the port in source code.

**Q: World not saving**  
A: Ensure the server has write permissions to the directory. Run chmod 755 on the server folder.

**Q: High CPU usage on TV**  
A: Reduce view distance in server.properties (set to 4 or 5). Also verify that only 1 chunk per tick is being sent (this fix is included).


## Original Project

This fork is based on [theorzr/mc173](https://github.com/theorzr/mc173). All credit for the original Minecraft Beta 1.7.3 server implementation goes to the original author. This fork only adds ARMv7/LG webOS compatibility fixes.
