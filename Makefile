ASM=nasm
SRC_DIR=src
BUILD_DIR=build

.PHONY: all floppy_image clean run disk_image user_programs $(BUILD_DIR)/KERNEL.BIN

all: floppy_image

floppy_image: $(BUILD_DIR)/main_floppy.img

$(BUILD_DIR)/main_floppy.img: $(BUILD_DIR)/main.bin $(BUILD_DIR)/stage2.bin $(BUILD_DIR)/KERNEL.BIN
	# Create a blank 1.44MB floppy image
	dd if=/dev/zero of=$(BUILD_DIR)/main_floppy.img bs=512 count=2880
	
	# Format it as FAT12
	mkfs.fat -F 12 -n "NBOS" $(BUILD_DIR)/main_floppy.img
	
	# Write bootloader to the first sector
	dd if=$(BUILD_DIR)/main.bin of=$(BUILD_DIR)/main_floppy.img conv=notrunc bs=512 count=1
	
	# Copy files to the image using mcopy
	mcopy -i $(BUILD_DIR)/main_floppy.img $(BUILD_DIR)/stage2.bin ::STAGE2.BIN
	mcopy -i $(BUILD_DIR)/main_floppy.img $(BUILD_DIR)/KERNEL.BIN ::KERNEL.BIN

$(BUILD_DIR)/KERNEL.BIN:
	mkdir -p $(BUILD_DIR)
	cd kernel && PATH="$$HOME/.cargo/bin:$$PATH" RUSTFLAGS="-C target-feature=-redzone -C link-arg=-Tlinker.ld -C relocation-model=static" cargo build --target x86_64-unknown-none --release
	objcopy -O binary kernel/target/x86_64-unknown-none/release/kernel $(BUILD_DIR)/KERNEL.BIN

$(BUILD_DIR)/main.bin: $(SRC_DIR)/main.asm
	mkdir -p $(BUILD_DIR)
	$(ASM) $(SRC_DIR)/main.asm -f bin -o $(BUILD_DIR)/main.bin

$(BUILD_DIR)/stage2.bin: $(SRC_DIR)/stage2.asm
	mkdir -p $(BUILD_DIR)
	$(ASM) $(SRC_DIR)/stage2.asm -f bin -o $(BUILD_DIR)/stage2.bin

# Build user-space ELF programs
user_programs: $(BUILD_DIR)/hello.elf

$(BUILD_DIR)/hello.elf: user/hello.rs
	mkdir -p $(BUILD_DIR)
	$$HOME/.cargo/bin/rustc --edition 2021 -C panic=abort \
		--target x86_64-unknown-none \
		-C code-model=large \
		-C relocation-model=static \
		-C link-arg=-nostdlib \
		-C link-arg=--image-base=0x8000000000 \
		-C opt-level=2 \
		-o $(BUILD_DIR)/hello.elf \
		user/hello.rs



$(BUILD_DIR)/disk.img:
	mkdir -p $(BUILD_DIR)
	dd if=/dev/zero of=$(BUILD_DIR)/disk.img bs=1M count=10
	mkfs.fat -F 32 $(BUILD_DIR)/disk.img
	echo "Hello from MYNEWOS FAT32 disk!" > /tmp/hello.txt
	mcopy -i $(BUILD_DIR)/disk.img /tmp/hello.txt ::hello.txt
	echo "Welcome to the Shell. Type help for commands." > /tmp/readme.txt
	mcopy -i $(BUILD_DIR)/disk.img /tmp/readme.txt ::readme.txt
	# Copy the user-space ELF binaries
	if [ -f $(BUILD_DIR)/hello.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/hello.elf ::hello.elf; fi


disk_image: $(BUILD_DIR)/disk.img

run: floppy_image disk_image
	qemu-system-x86_64 -fda $(BUILD_DIR)/main_floppy.img -hda $(BUILD_DIR)/disk.img -smp 2 -boot order=a -serial stdio

run-with-user: user_programs floppy_image disk_image
	qemu-system-x86_64 -fda $(BUILD_DIR)/main_floppy.img -hda $(BUILD_DIR)/disk.img -smp 2 -boot order=a -serial stdio

clean:
	rm -rf $(BUILD_DIR)
	cd kernel && cargo clean
