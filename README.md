# mc173webos guide 

```sh
sudo apt update
sudo apt install curl git build-essential
```


1. Install `rustup`:

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```


   ```sh
   source ~/.cargo/env
   ```

4. Verity the installation:

   ```sh
   rustc --version
   cargo --version
   ```



```sh
rustup target add armv7-unknown-linux-gnueabihf
rustup target add armv7-unknown-linux-musleabihf
```

Verity:

```sh
rustup target list --installed
```

You should see both `armv7-unknown-linux-gnueabihf` and `armv7-unknown-linux-musleabihf` listed.

## Step 3: Install the ARM GCC Cross-Linker

Rust needs a cross-linker to link ARM binaries.

```sh
sudo apt install gcc-arm-linux-gnueabihf
```

Verify:

```sh
arm-linux-gnueabihf-gcc --version
```

Example output (the version depends on your distro):

```text
arm-linux-gnueabihf-gcc (Ubuntu 13.3.0-6ubuntu2~24.04) 13.3.0
```

If the package cannot be found, run `sudo apt update` and try again.

## Step 4: Clone the Repository

```sh
git clone https://github.com/ballslober12/mc173-webos.git
cd mc173-webos
ls -la
```

You should see `Cargo.toml`, `mc173/`, `mc173-server/`, and `README.md`.

## Step 5: Configure Cargo for Cross-Compilation

Tell Cargo which linker to use for each target:

```sh
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"

[target.armv7-unknown-linux-musleabihf]
linker = "arm-linux-gnueabihf-gcc"
EOF
```

Verify:

```sh
cat .cargo/config.toml
```

## Step 6: Build the Server (Dynamic)

```sh
cargo build --release --target armv7-unknown-linux-gnueabihf
```

This typically takes a few minutes. The binary is dynamically linked against glibc, so the target device needs a compatible glibc version.

Output: `target/armv7-unknown-linux-gnueabihf/release/`

## Step 7: Build the Server (Static, Recommended)

```sh
cargo build --release --target armv7-unknown-linux-musleabihf
```

This produces a statically linked binary that does not depend on the target's libc, so it runs on most ARMv7 hard-float Linux systems. It may take a few minutes longer than the dynamic build.

Output: `target/armv7-unknown-linux-musleabihf/release/`

> **Note:** Pure-Rust dependencies build fine with this setup. If a dependency compiles C code, you may need a musl cross-toolchain or the [`cross`](https://github.com/cross-rs/cross) tool instead.

## Step 8: Verify the Binary

```sh
file target/armv7-unknown-linux-musleabihf/release/mc173-server
```

The output should mention `ELF 32-bit LSB executable, ARM, EABI5`. Copy the binary to your device (for example with `scp`) and run it there.

> The binary name above assumes the server crate's binary is called `mc173-server`; check `ls target/<target>/release/` if it differs.

im sorry i did this readme with claude
