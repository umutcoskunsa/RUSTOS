#!/bin/bash
# populate_ext2.sh - Script to create a test ext2 image with various file sizes

IMG="build/data.img"
MNT="/tmp/ext2_mnt"

mkdir -p build
dd if=/dev/zero of=$IMG bs=1M count=10
mkfs.ext2 -F $IMG

mkdir -p $MNT
# We need sudo to mount. If sudo is not available, we can use e2tools or just skip.
# But usually WSL has sudo.
if sudo mount -o loop $IMG $MNT; then
    sudo mkdir -p $MNT/test_dir
    echo "Small file content" | sudo tee $MNT/small.txt > /dev/null
    
    # Large file (> 12KB) to test indirect blocks
    # 64KB file = 16 blocks (4KB each)
    sudo dd if=/dev/urandom of=$MNT/large.bin bs=1k count=64
    
    # Text file that is exactly 20KB
    sudo seq 1 4000 | sudo tee $MNT/numbers.txt > /dev/null
    
    sudo umount $MNT
    echo "Disk populated successfully."
else
    echo "Failed to mount disk image. Please ensure you have sudo privileges or use e2tools."
fi
