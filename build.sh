#!/bin/bash

# Add targets if not already added
rustup target add x86_64-pc-windows-gnu
rustup target add x86_64-unknown-linux-gnu
rustup target add x86_64-apple-darwin

# Build for each target
cargo build --target x86_64-pc-windows-gnu --release
cargo build --target x86_64-unknown-linux-gnu --release
cargo build --target x86_64-apple-darwin --release

echo "Build completed for Windows, Linux, and macOS."
