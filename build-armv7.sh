#!/usr/bin/env bash
#
# build-armv7.sh — cross-compile mc173-server for armv7l (32-bit ARM, e.g. webOS TV).
#
# Usage:
#   ./build-armv7.sh            # dynamic build (armv7-unknown-linux-gnueabihf)
#   ./build-armv7.sh --static   # static musl build (armv7-unknown-linux-musleabihf)
#                                 recommended by the README: no external libs needed
#                                 on the target device.
#   ./build-armv7.sh --check-only   # just verify the toolchain, don't build
#
# What it does, matching README.md steps 1-7:
#   1. Checks for rustup/cargo, and adds the armv7 target if missing.
#   2. Checks for the ARM cross-compiler (gcc-arm-linux-gnueabihf, or musl-cross for
#      the static build) and gives you the exact apt-get command if it's missing.
#   3. Writes/updates .cargo/config.toml with the right linker for whichever target
#      you're building, without touching any other targets already configured there
#      (e.g. the x86_64 entry used for local `cargo check`/dev builds).
#   4. Runs `cargo build --release --target <target>`.
#   5. Copies the resulting binary into ./dist/ and prints its size, so you can see
#      at a glance whether `strip = true` in Cargo.toml's [profile.release] actually
#      took effect.
#
# Safe to re-run: steps are idempotent (adding an already-installed target, or an
# already-present config.toml block, is a no-op).

set -euo pipefail

# --- Parse arguments ---------------------------------------------------------

STATIC=0
CHECK_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --static) STATIC=1 ;;
        --check-only) CHECK_ONLY=1 ;;
        -h|--help)
            sed -n '2,20p' "$0"
            exit 0
            ;;
        *)
            echo "Unknown argument: $arg (use --static, --check-only, or --help)" >&2
            exit 1
            ;;
    esac
done

if [ "$STATIC" -eq 1 ]; then
    TARGET="armv7-unknown-linux-musleabihf"
    LINKER_CMD="arm-linux-musleabihf-gcc"
    APT_PKG=""   # musl-cross toolchains aren't in the default Ubuntu/Debian repos
else
    TARGET="armv7-unknown-linux-gnueabihf"
    LINKER_CMD="arm-linux-gnueabihf-gcc"
    APT_PKG="gcc-arm-linux-gnueabihf"
fi

# Must be run from the project root (where Cargo.toml lives).
if [ ! -f "Cargo.toml" ] || [ ! -d "mc173-server" ]; then
    echo "Error: run this from the project root (the directory with Cargo.toml and mc173-server/)." >&2
    exit 1
fi

echo "==> Target: $TARGET"

# --- Step 1: rustup + target ------------------------------------------------

if ! command -v rustup >/dev/null 2>&1; then
    echo "Error: rustup not found." >&2
    echo "Install it with:" >&2
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    echo "  source \"\$HOME/.cargo/env\"" >&2
    exit 1
fi

if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "==> Installing Rust target $TARGET ..."
    rustup target add "$TARGET"
else
    echo "==> Rust target $TARGET already installed."
fi

# --- Step 2: ARM cross-compiler ---------------------------------------------

if ! command -v "$LINKER_CMD" >/dev/null 2>&1; then
    echo "Error: $LINKER_CMD not found on PATH." >&2
    if [ "$STATIC" -eq 1 ]; then
        echo "The musl cross-toolchain isn't packaged for Ubuntu/Debian by default." >&2
        echo "Get a prebuilt one from https://musl.cc (look for armv7l-linux-musleabihf-cross)," >&2
        echo "extract it, and add its bin/ directory to your PATH, e.g.:" >&2
        echo "  export PATH=\"\$HOME/armv7l-linux-musleabihf-cross/bin:\$PATH\"" >&2
    else
        echo "Install it with:" >&2
        echo "  sudo apt update && sudo apt install $APT_PKG" >&2
    fi
    exit 1
else
    echo "==> Found cross-compiler: $(command -v "$LINKER_CMD")"
fi

# --- Step 3: .cargo/config.toml ----------------------------------------------

mkdir -p .cargo
CONFIG=".cargo/config.toml"
touch "$CONFIG"

if grep -q "^\[target\.$TARGET\]" "$CONFIG" 2>/dev/null; then
    echo "==> .cargo/config.toml already has a [target.$TARGET] entry, leaving it as-is."
else
    echo "==> Adding [target.$TARGET] to $CONFIG"
    {
        echo ""
        echo "[target.$TARGET]"
        echo "linker = \"$LINKER_CMD\""
    } >> "$CONFIG"
fi

if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "==> --check-only given, toolchain looks good. Skipping build."
    exit 0
fi

# --- Step 4: build ------------------------------------------------------------

echo "==> Building (release, $TARGET) ..."
cargo build --release --target "$TARGET"

# --- Step 5: collect output ----------------------------------------------------

BIN_SRC="target/$TARGET/release/mc173-server"
if [ ! -f "$BIN_SRC" ]; then
    echo "Error: expected binary not found at $BIN_SRC (check the build output above)." >&2
    exit 1
fi

mkdir -p dist
BIN_DST="dist/mc173-server-$TARGET"
cp "$BIN_SRC" "$BIN_DST"

echo ""
echo "==> Build complete: $BIN_DST"
ls -lh "$BIN_DST"
file "$BIN_DST" 2>/dev/null || true
