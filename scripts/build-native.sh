#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${HOME}/.cargo/bin:$PATH"
cargo build --locked --release -p nes-android --target aarch64-linux-android
mkdir -p android/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/release/libnes_android.so android/app/src/main/jniLibs/arm64-v8a/

# x86_64 is included in debug APKs for emulator smoke tests.
cargo build --locked --release -p nes-android --target x86_64-linux-android
mkdir -p android/app/src/main/jniLibs/x86_64
cp target/x86_64-linux-android/release/libnes_android.so android/app/src/main/jniLibs/x86_64/
