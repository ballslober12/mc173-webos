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

Open terminal and run:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
When prompted, select option 1: "Proceed with installation (default)"

After installation, reload your shell:

bash
source ~/.cargo/env
Verify Rust is installed:

bash
rustc --version
cargo --version
Expected output example:

text
rustc 1.85.0 (some date)
cargo 1.85.0 (some date)
Step 2: Install ARM cross-compilation target
bash
rustup target add armv7-unknown-linux-gnueabihf
Verify target is added:

bash
rustup target list | grep armv7
You should see armv7-unknown-linux-gnueabihf (installed)

Step 3: Install GCC cross-compiler for ARM
bash
sudo apt update
sudo apt install gcc-arm-linux-gnueabihf
Verify installation:

bash
arm-linux-gnueabihf-gcc --version
Expected output: arm-linux-gnueabihf-gcc (Ubuntu 13.3.0-...) 13.3.0

Step 4: Clone this repository
bash
git clone https://github.com/ballslober12/mc173-webos.git
cd mc173-webos
Step 5: Configure Cargo for cross-compilation
Create a file .cargo/config.toml in the project root:

bash
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"
EOF
Step 6: Build the server (DYNAMIC build)
bash
cargo build --release --target armv7-unknown-linux-gnueabihf
This will take 2-5 minutes. The binary will be approximately 4-5 MB.

Step 7: Build the server (STATIC build - RECOMMENDED for LG TV)
Static build requires musl target:

bash
rustup target add armv7-unknown-linux-musleabihf
cargo build --release --target armv7-unknown-linux-musleabihf
The static binary will be approximately 10-15 MB but runs on ANY ARM Linux without external libraries.

Step 8: Locate the compiled binary
For dynamic build:

bash
ls -lh target/armv7-unknown-linux-gnueabihf/release/mc173-server
For static build:

bash
ls -lh target/armv7-unknown-linux-musleabihf/release/mc173-server
Step 9: Check binary architecture (important!)
bash
file target/armv7-unknown-linux-gnueabihf/release/mc173-server
Expected output should contain: ELF 32-bit LSB executable, ARM

If it shows x86-64 - wrong build. Check Step 2 and Step 5.

Step 10: Copy binary to USB drive
Assuming your USB drive is mounted at /mnt/e/:

bash
cp target/armv7-unknown-linux-gnueabihf/release/mc173-server /mnt/e/
If your USB has different mount point, check with:

bash
ls /mnt/
Common mount points: /mnt/d/, /mnt/f/, /media/username/

Running on TV
Step 1: Copy binary from USB to TV
On LG TV via SSH:

bash
cd /home/root
mkdir -p minecraft_server
cd minecraft_server
cp /media/internal/mc173-server .
Note: USB path may vary. Common paths on LG TV:

/media/internal/

/media/usb0/

/tmp/usb/

Step 2: Make binary executable
bash
chmod +x mc173-server
Step 3: Run the server
bash
./mc173-server
Step 4: Expected output
If successful, you will see:

text
INFO server bound to 0.0.0.0:25565
DEBUG loaded chunk from storage: -10/-10
DEBUG loaded chunk from storage: -10/-9
...
Step 5: If "not found" error
If you get -sh: ./mc173-server: not found, try:

bash
/lib/ld-linux.so.3 ./mc173-server
Or create a symlink:

bash
ln -s /lib/ld-linux.so.3 /lib/ld-linux-armhf.so.3
./mc173-server
Step 6: Keep server running in background
bash
nohup ./mc173-server > server.log 2>&1 &
To view logs:

bash
tail -f server.log
To stop server:

bash
pkill mc173-server
Connecting from PC
Step 1: Find TV IP address
On TV via SSH:

bash
ip addr show | grep "inet "
Look for something like 192.168.1.100
