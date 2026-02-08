#!/usr/bin/env bash
# ABOUTME: Builds and boots MobileOS in QEMU aarch64.
# ABOUTME: Cross-compiles initd, assembles an initramfs with busybox, and launches QEMU.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BUILD_DIR="$ROOT_DIR/target/qemu-image"
KERNEL_DIR="$BUILD_DIR/kernel"
INITRAMFS_DIR="$BUILD_DIR/initramfs"
TARGET="aarch64-unknown-linux-gnu"
PROFILE="${1:-debug}"

# Kernel version to download (pre-built from Debian/Ubuntu)
KERNEL_VERSION="6.6.0"
KERNEL_URL="https://cloud-images.ubuntu.com/noble/current/unpacked/noble-server-cloudimg-arm64-vmlinuz-generic"

echo "=== MobileOS QEMU Image Builder ==="
echo "Build dir: $BUILD_DIR"
echo "Profile:   $PROFILE"

mkdir -p "$BUILD_DIR" "$KERNEL_DIR" "$INITRAMFS_DIR"

# --- Step 1: Cross-compile initd ---
echo ""
echo "--- Building mos-initd for $TARGET ---"
if [ "$PROFILE" = "release" ]; then
    cargo build -p mos-initd --target "$TARGET" --release
    INIT_BIN="$ROOT_DIR/target/$TARGET/release/mos-initd"
else
    cargo build -p mos-initd --target "$TARGET"
    INIT_BIN="$ROOT_DIR/target/$TARGET/debug/mos-initd"
fi
echo "Built: $INIT_BIN"

# --- Step 2: Get kernel ---
KERNEL_IMAGE="$KERNEL_DIR/vmlinuz"
if [ ! -f "$KERNEL_IMAGE" ]; then
    echo ""
    echo "--- Downloading aarch64 kernel ---"
    # Use a pre-built kernel from Alpine Linux (smaller, simpler)
    ALPINE_KERNEL_URL="https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/aarch64/netboot/vmlinuz-lts"
    curl -fSL -o "$KERNEL_IMAGE" "$ALPINE_KERNEL_URL"
    echo "Downloaded kernel to $KERNEL_IMAGE"
else
    echo "Kernel already present: $KERNEL_IMAGE"
fi

# --- Step 3: Get static busybox for the initramfs ---
BUSYBOX="$BUILD_DIR/busybox"
if [ ! -f "$BUSYBOX" ]; then
    echo ""
    echo "--- Downloading static busybox (aarch64) ---"
    BUSYBOX_URL="https://busybox.net/downloads/binaries/1.35.0-aarch64-linux-musl/busybox"
    curl -fSL -o "$BUSYBOX" "$BUSYBOX_URL"
    chmod +x "$BUSYBOX"
    echo "Downloaded busybox to $BUSYBOX"
else
    echo "Busybox already present: $BUSYBOX"
fi

# --- Step 4: Assemble initramfs ---
echo ""
echo "--- Assembling initramfs ---"
rm -rf "$INITRAMFS_DIR"
mkdir -p "$INITRAMFS_DIR"/{bin,sbin,dev,proc,sys,tmp,run,etc,lib}

# Install our init as /init (what the kernel looks for)
cp "$INIT_BIN" "$INITRAMFS_DIR/init"

# Install busybox and create symlinks
cp "$BUSYBOX" "$INITRAMFS_DIR/bin/busybox"
# Create essential symlinks
for cmd in sh ls cat echo mkdir mount umount ps kill sleep; do
    ln -sf busybox "$INITRAMFS_DIR/bin/$cmd"
done

# Create minimal /dev nodes
# (devtmpfs will be mounted by init, but we need console for early boot)
pushd "$INITRAMFS_DIR/dev" > /dev/null
mknod -m 622 console c 5 1 2>/dev/null || true
mknod -m 666 null c 1 3 2>/dev/null || true
mknod -m 666 zero c 1 5 2>/dev/null || true
mknod -m 444 random c 1 8 2>/dev/null || true
mknod -m 444 urandom c 1 9 2>/dev/null || true
mknod -m 666 tty c 5 0 2>/dev/null || true
popd > /dev/null

# Create initramfs cpio archive
INITRAMFS_CPIO="$BUILD_DIR/initramfs.cpio.gz"
pushd "$INITRAMFS_DIR" > /dev/null
find . | cpio -o -H newc 2>/dev/null | gzip > "$INITRAMFS_CPIO"
popd > /dev/null
echo "Created initramfs: $INITRAMFS_CPIO"

# --- Step 5: Get aarch64 shared libraries for dynamically linked binary ---
# The init binary is dynamically linked, so we need the aarch64 libc
LIBC_DIR="$BUILD_DIR/aarch64-libs"
if [ ! -d "$LIBC_DIR" ]; then
    echo ""
    echo "--- Collecting aarch64 shared libraries ---"
    mkdir -p "$LIBC_DIR"
    # Copy from the cross-compilation toolchain
    SYSROOT=$(aarch64-linux-gnu-gcc -print-sysroot 2>/dev/null || echo "/usr/aarch64-linux-gnu")
    if [ -d "$SYSROOT/lib" ]; then
        cp -a "$SYSROOT/lib/ld-linux-aarch64.so.1" "$LIBC_DIR/" 2>/dev/null || true
        cp -a "$SYSROOT/lib/aarch64-linux-gnu/libc.so.6" "$LIBC_DIR/" 2>/dev/null || true
        cp -a "$SYSROOT/lib/aarch64-linux-gnu/libgcc_s.so.1" "$LIBC_DIR/" 2>/dev/null || true
        cp -a "$SYSROOT/lib/aarch64-linux-gnu/libm.so.6" "$LIBC_DIR/" 2>/dev/null || true
    fi
fi

# Add libs to initramfs if they exist
if [ -d "$LIBC_DIR" ] && ls "$LIBC_DIR"/*.so* 1>/dev/null 2>&1; then
    mkdir -p "$INITRAMFS_DIR/lib"
    cp -a "$LIBC_DIR"/* "$INITRAMFS_DIR/lib/"
    # Also create the standard symlink location
    mkdir -p "$INITRAMFS_DIR/lib/aarch64-linux-gnu"
    for f in "$LIBC_DIR"/*; do
        ln -sf "../$(basename "$f")" "$INITRAMFS_DIR/lib/aarch64-linux-gnu/$(basename "$f")" 2>/dev/null || true
    done
    # Rebuild initramfs with libs
    pushd "$INITRAMFS_DIR" > /dev/null
    find . | cpio -o -H newc 2>/dev/null | gzip > "$INITRAMFS_CPIO"
    popd > /dev/null
    echo "Rebuilt initramfs with shared libraries"
fi

# --- Step 6: Launch QEMU ---
echo ""
echo "=== Launching QEMU ==="
echo "Press Ctrl+A then X to exit QEMU"
echo ""

exec qemu-system-aarch64 \
    -machine virt \
    -cpu cortex-a53 \
    -m 512M \
    -nographic \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$INITRAMFS_CPIO" \
    -append "console=ttyAMA0 rdinit=/init" \
    -no-reboot
