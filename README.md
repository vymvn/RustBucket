RustBucket
---

A simple C2 framework written in Rust.

This is basically learning project for me and my team, to learn Rust and how C2 frameworks work. It is in no way a finished project that's ready for a production environment.

# Requirements

## Rust

Follow [the official instructions](https://www.rust-lang.org/tools/install) to install Rust.

## Mingw

Mingw is needed to cross compile the windows payload. You can install it using your package manager.

### Debian Based Distros (Ubuntu, Kali, etc)

```bash
sudo apt update
sudo apt install mingw-w64
```


### Arch Linux / Arch Based Distros (Manjaro, etc)

```bash
sudo pacman -S mingw-w64-gcc
```

### RH Based Distros (Fedora, CentOS, etc)

```bash
sudo dnf install mingw32-gcc mingw64-gcc
```

## Cargo cross compilation capabilities

You also need to add windows cross compilation features to cargo to be able to cross compile the windows payload.

```bash
rustup target add x86_64-pc-windows-gnu
```

# Build

To build the entire project, you can use the following command:

```bash
cargo build --release
```

This builds all the binaries in the `target/release` folder.


To build a specific crate, you can use the following command for crates with the `main.rs` entry point:

```bash
cargo build --bin <crate_name> --release
```

Or for crates with a `lib.rs` entry point:

```bash
cargo build --lib <crate_name> --release
```

# Usage

1. Run the server

```bash
./rb_server
```

Runs the server on default port 6666. This can be customized later when we add config files.

2. Run the client

```bash
./rb_client
```

This will connect to the default server on localhost:6666. You can change the connection details with command line flags. Run `./rb_client --help` for more details.


# TODO

- Cleaner state management (and cleaner code in general).
- Implement actual commands to do stuff instead of just powershell commands.
- Nicer cli experience (tab completion, syntax highlighting, etc).
- A DB to save server state.
- Config file for server.
- Operator profiles and a command to generate them.
- Make server send the generated payload to the client (currently it just stays on the server).
