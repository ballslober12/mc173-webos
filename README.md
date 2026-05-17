To install Rust and set up your environment for ARM cross-compilation, follow these steps:

▎Step 1: Install Rust

1. Open your terminal and run:

      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   

2. When prompted, select option 1: "Proceed with installation (default)".

3. After installation completes, reload your shell configuration:

      source ~/.cargo/env
   

4. Verify Rust is installed correctly:

      rustc --version
   

   Expected output example:

      rustc 1.85.0 (4cb91f7a7 2025-02-17)
   

5. Verify Cargo is installed:

      cargo --version
   

   Expected output example:

      cargo 1.85.0 (4cb91f7a7 2025-02-17)
   

6. If you see command not found, restart your terminal or run:

      echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
   source ~/.bashrc
   

▎Step 2: Install ARM Cross-Compilation Target

1. Add the ARM target:

      rustup target add armv7-unknown-linux-gnueabihf
   

   Expected output:

      info: downloading component 'rust-std' for 'armv7-unknown-linux-gnueabihf'
   info: installing component 'rust-std' for 'armv7-unknown-linux-gnueabihf'
   

2. Verify the target is installed:

      rustup target list | grep armv7 | grep installed
   

   You should see:

      armv7-unknown-linux-gnueabihf (installed)
   

▎Step 3: Install GCC Cross-Compiler for ARM

On Ubuntu/Debian/WSL:

1. Update your package list and install the ARM GCC compiler:

      sudo apt update
   sudo apt install gcc-arm-linux-gnueabihf
   

2. Verify installation:

      arm-linux-gnueabihf-gcc --version
   

   Expected output:

      arm-linux-gnueabihf-gcc (Ubuntu 13.3.0-... 13.3.0)
   

3. If you see command not found, try:

      sudo apt install gcc-arm-linux-gnueabihf --fix-missing
   

▎Step 4: Clone the Repository

1. Download the source code:

      git clone https://github.com/ballslober12/mc173-webos.git
   cd mc173-webos
   

2. Verify you are in the correct directory:

      ls -la
   

   You should see files like Cargo.toml, mc173/, mc173-server/, and README.md.

▎Step 5: Configure Cargo for Cross-Compilation

1. Create a configuration directory:

      mkdir -p .cargo
   

2. Create the config file with proper content:

      cat > .cargo/config.toml << 'EOF'
   [target.armv7-unknown-linux-gnueabihf]
   linker = "arm-linux-gnueabihf-gcc"
   EOF
   

3. Verify the file was created:

      cat .cargo/config.toml
   

   Expected output:

      [target.armv7-unknown-linux-gnueabihf]
   linker = "arm-linux-gnueabihf-gcc"
   

▎Step 6: Build the Server (DYNAMIC Build)

1. Run the build command:

      cargo build --release --target armv7-unknown-linux-gnueabihf
   

This will take 2-5 minutes depending on your computer speed, and you will see many lines of compilation output.

▎Step 7: Build the Server (STATIC Build - RECOMMENDED)

1. First, add the static musl target:

      rustup target add armv7-unknown-linux-musleabihf
   

2. Now build with the musl target:

      cargo build --release --target armv7-unknown-linux-musleabihf
   

This will take longer (3-7 minutes) and produce a larger binary that includes all dependencies inside, making it suitable for any ARM Linux environment without needing external libraries.
