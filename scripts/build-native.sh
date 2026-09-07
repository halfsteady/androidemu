#!/usr/bin/env bash
# Cross-compile the core for the tablet and drop the .so where Gradle packages
# it from. API 29 is the floor: the Storage Access Framework and the AAudio
# low-latency path both assume it, and the Pad 3 is far newer.
#
# The NDK toolchain is located at run time rather than pinned to one machine's
# path, so a CI runner works the same as this one. Cargo reads the
# CARGO_TARGET_<TRIPLE>_LINKER variables set here ahead of any .cargo config,
# which is why there is no committed config naming an absolute path.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="${HOME}/.cargo/bin:$PATH"

api=29
ndk_version=28.2.13676358
sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-$sdk/ndk/$ndk_version}}"
host=linux-x86_64
[ "$(uname -s)" = "Darwin" ] && host=darwin-x86_64
bin="$ndk/toolchains/llvm/prebuilt/$host/bin"
if [ ! -x "$bin/clang" ]; then
    echo "No NDK toolchain at $bin" >&2
    echo "Install NDK $ndk_version under \$ANDROID_HOME/ndk/, or set ANDROID_NDK_HOME." >&2
    exit 1
fi
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$bin/aarch64-linux-android$api-clang"
export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$bin/armv7a-linux-androideabi$api-clang"
export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$bin/x86_64-linux-android$api-clang"

build() {
    cargo build --locked --release -p nes-android --target "$1"
    mkdir -p "android/app/src/main/jniLibs/$2"
    cp "target/$1/release/libnes_android.so" "android/app/src/main/jniLibs/$2/"
}

build aarch64-linux-android arm64-v8a
# x86_64 is included in debug APKs for emulator smoke tests.
build x86_64-linux-android x86_64
