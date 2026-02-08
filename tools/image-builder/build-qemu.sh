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

echo "=== MobileOS QEMU Image Builder ==="
echo "Build dir: $BUILD_DIR"
echo "Profile:   $PROFILE"

mkdir -p "$BUILD_DIR" "$KERNEL_DIR"

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
    curl -fSL -o "$KERNEL_IMAGE" \
        "https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/aarch64/netboot/vmlinuz-lts"
    echo "Downloaded kernel to $KERNEL_IMAGE"
else
    echo "Kernel already present: $KERNEL_IMAGE"
fi

# --- Step 3: Get static busybox for the initramfs ---
BUSYBOX="$BUILD_DIR/busybox"
if [ ! -f "$BUSYBOX" ]; then
    echo ""
    echo "--- Downloading static busybox (aarch64) from Alpine ---"
    BUSYBOX_APK="$BUILD_DIR/busybox-static.apk"
    curl -fSL -o "$BUSYBOX_APK" \
        "https://dl-cdn.alpinelinux.org/alpine/v3.21/main/aarch64/busybox-static-1.37.0-r14.apk"
    TMP_EXTRACT="$BUILD_DIR/busybox-extract"
    mkdir -p "$TMP_EXTRACT"
    tar xzf "$BUSYBOX_APK" -C "$TMP_EXTRACT"
    cp "$TMP_EXTRACT/bin/busybox.static" "$BUSYBOX"
    chmod +x "$BUSYBOX"
    rm -rf "$TMP_EXTRACT" "$BUSYBOX_APK"
    echo "Downloaded busybox to $BUSYBOX"
else
    echo "Busybox already present: $BUSYBOX"
fi

# --- Step 4: Collect aarch64 shared libraries ---
AARCH64_LIBDIR="/usr/aarch64-linux-gnu/lib"
REQUIRED_LIBS=(
    "$AARCH64_LIBDIR/ld-linux-aarch64.so.1"
    "$AARCH64_LIBDIR/libc.so.6"
    "$AARCH64_LIBDIR/libgcc_s.so.1"
)
for lib in "${REQUIRED_LIBS[@]}"; do
    if [ ! -f "$lib" ]; then
        echo "ERROR: Required library not found: $lib"
        echo "Install gcc-aarch64-linux-gnu to get the aarch64 sysroot."
        exit 1
    fi
done

# --- Step 5: Assemble initramfs ---
echo ""
echo "--- Assembling initramfs ---"
rm -rf "$INITRAMFS_DIR"
mkdir -p "$INITRAMFS_DIR"/{bin,sbin,dev,proc,sys,tmp,run,etc,lib}

# Our init as /init (what the kernel executes)
cp "$INIT_BIN" "$INITRAMFS_DIR/init"

# Busybox and essential command symlinks
cp "$BUSYBOX" "$INITRAMFS_DIR/bin/busybox"
for cmd in sh ls cat echo mkdir mount umount ps kill sleep; do
    ln -sf busybox "$INITRAMFS_DIR/bin/$cmd"
done

# Shared libraries for the dynamically linked init
for lib in "${REQUIRED_LIBS[@]}"; do
    cp "$lib" "$INITRAMFS_DIR/lib/"
done

# Overlay rootfs static files (service configs, etc.)
if [ -d "$ROOT_DIR/rootfs" ]; then
    cp -a "$ROOT_DIR/rootfs/"* "$INITRAMFS_DIR/" 2>/dev/null || true
    echo "Installed rootfs overlay"
fi

# Minimal /dev nodes for early boot (devtmpfs takes over once mounted)
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
echo "Created initramfs: $INITRAMFS_CPIO ($(du -h "$INITRAMFS_CPIO" | cut -f1))"

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
