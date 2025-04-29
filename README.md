RustBucket
---

A simple C2 framework written in Rust.

This is basically learning project for me and my team, to learn Rust and how C2 frameworks work. It is in no way a finished project that's ready for a production environment.

# TODO

- Cleaner state management (and cleaner code in general).
- Implement actual commands to do stuff instead of just powershell commands.
- Nicer cli experience (tab completion, syntax highlighting, etc).
- A DB to save server state.
- Config file for server.
- Operator profiles and a command to generate them.
- Fix TLS lol
- Real implant??
- Make server send the generated payload to the client (currently it just stays on the server).

# Requirements

You need mingw

and add windows cross compilation features to cargo to be able to cross compile the windows payload.

```
rustup target add x86_64-pc-windows-gnu
```

# Build

```bash
cargo build --release
```

This builds all the binaries in the `target/release` folder.
