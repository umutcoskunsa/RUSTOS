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
	cd kernel && PATH="$$HOME/.cargo/bin:$$PATH" RUSTFLAGS="-C link-arg=-Tlinker.ld -C relocation-model=static -A warnings" cargo build --target x86_64-unknown-none --release
	objcopy -O binary kernel/target/x86_64-unknown-none/release/kernel $(BUILD_DIR)/KERNEL.BIN

$(BUILD_DIR)/main.bin: $(SRC_DIR)/main.asm
	mkdir -p $(BUILD_DIR)
	$(ASM) $(SRC_DIR)/main.asm -f bin -o $(BUILD_DIR)/main.bin

$(BUILD_DIR)/stage2.bin: $(SRC_DIR)/stage2.asm
	mkdir -p $(BUILD_DIR)
	$(ASM) $(SRC_DIR)/stage2.asm -f bin -o $(BUILD_DIR)/stage2.bin

# Build user-space ELF programs
user_programs: libc_build $(BUILD_DIR)/hello.elf $(BUILD_DIR)/loop.elf $(BUILD_DIR)/parent.elf $(BUILD_DIR)/child.elf $(BUILD_DIR)/hello_c.elf $(BUILD_DIR)/iotest.elf $(BUILD_DIR)/keytest.elf

libc_build: libc/libc.a libc/src/crt0.o

libc/libc.a libc/src/crt0.o:
	$(MAKE) -C libc

$(BUILD_DIR)/hello.elf: user/hello.rs user/linker.ld
	mkdir -p $(BUILD_DIR)
	$$HOME/.cargo/bin/rustc --edition 2021 -C panic=abort \
		--target x86_64-unknown-none \
		-C code-model=large \
		-C relocation-model=static \
		-C link-arg=-Tuser/linker.ld \
		-C link-arg=-nostdlib \
		-C opt-level=2 \
		-C strip=symbols \
		-o $(BUILD_DIR)/hello.elf \
		user/hello.rs

$(BUILD_DIR)/loop.elf: user/loop.rs user/linker.ld
	mkdir -p $(BUILD_DIR)
	$$HOME/.cargo/bin/rustc --edition 2021 -C panic=abort \
		--target x86_64-unknown-none \
		-C code-model=large \
		-C relocation-model=static \
		-C link-arg=-Tuser/linker.ld \
		-C link-arg=-nostdlib \
		-C opt-level=2 \
		-C strip=symbols \
		-o $(BUILD_DIR)/loop.elf \
		user/loop.rs

$(BUILD_DIR)/parent.elf: user/parent.rs user/linker.ld
	mkdir -p $(BUILD_DIR)
	$$HOME/.cargo/bin/rustc --edition 2021 -C panic=abort \
		--target x86_64-unknown-none \
		-C code-model=large \
		-C relocation-model=static \
		-C link-arg=-Tuser/linker.ld \
		-C link-arg=-nostdlib \
		-C opt-level=2 \
		-C strip=symbols \
		-o $(BUILD_DIR)/parent.elf \
		user/parent.rs

$(BUILD_DIR)/child.elf: user/child.rs user/linker.ld
	mkdir -p $(BUILD_DIR)
	$$HOME/.cargo/bin/rustc --edition 2021 -C panic=abort \
		--target x86_64-unknown-none \
		-C code-model=large \
		-C relocation-model=static \
		-C link-arg=-Tuser/linker.ld \
		-C link-arg=-nostdlib \
		-C opt-level=2 \
		-C strip=symbols \
		-o $(BUILD_DIR)/child.elf \
		user/child.rs

$(BUILD_DIR)/hello_c.elf: user/hello_c.c libc/libc.a libc/src/crt0.o user/linker.ld
	mkdir -p $(BUILD_DIR)
	x86_64-linux-gnu-gcc -Wall -Wextra -ffreestanding -nostdlib -static \
		-Ilibc/include -mcmodel=large -fno-stack-protector -mno-red-zone \
		-Wl,--build-id=none \
		-Tuser/linker.ld libc/src/crt0.o user/hello_c.c libc/libc.a -o $(BUILD_DIR)/hello_c.elf

$(BUILD_DIR)/iotest.elf: user/iotest.c libc/libc.a libc/src/crt0.o user/linker.ld
	mkdir -p $(BUILD_DIR)
	x86_64-linux-gnu-gcc -Wall -Wextra -ffreestanding -nostdlib -static \
		-Ilibc/include -mcmodel=large -fno-stack-protector -mno-red-zone \
		-Wl,--build-id=none \
		-Tuser/linker.ld libc/src/crt0.o user/iotest.c libc/libc.a -o $(BUILD_DIR)/iotest.elf

$(BUILD_DIR)/keytest.elf: user/keytest.c libc/libc.a libc/src/crt0.o user/linker.ld
	mkdir -p $(BUILD_DIR)
	x86_64-linux-gnu-gcc -Wall -Wextra -ffreestanding -nostdlib -static \
		-Ilibc/include -mcmodel=large -fno-stack-protector -mno-red-zone \
		-Wl,--build-id=none \
		-Tuser/linker.ld libc/src/crt0.o user/keytest.c libc/libc.a -o $(BUILD_DIR)/keytest.elf

# Rule to build the DOOM binary using its own sub-Makefile
doom/doomgeneric-master/doomgeneric/doom.elf:
	$(MAKE) -C doom/doomgeneric-master/doomgeneric -f Makefile.mynewos

$(BUILD_DIR)/disk.img: $(BUILD_DIR)/hello.elf $(BUILD_DIR)/loop.elf $(BUILD_DIR)/parent.elf $(BUILD_DIR)/child.elf $(BUILD_DIR)/hello_c.elf $(BUILD_DIR)/iotest.elf $(BUILD_DIR)/keytest.elf doom/doomgeneric-master/doomgeneric/doom.elf
	mkdir -p $(BUILD_DIR)
	rm -f $(BUILD_DIR)/disk.img
	dd if=/dev/zero of=$(BUILD_DIR)/disk.img bs=1M count=10
	mkfs.fat -F 32 $(BUILD_DIR)/disk.img
	echo "Hello from MYNEWOS FAT32 disk!" > /tmp/hello.txt
	mcopy -i $(BUILD_DIR)/disk.img /tmp/hello.txt ::hello.txt
	echo "Welcome to the Shell. Type help for commands." > /tmp/readme.txt
	mcopy -i $(BUILD_DIR)/disk.img /tmp/readme.txt ::readme.txt
	# Copy the user-space ELF binaries
	if [ -f $(BUILD_DIR)/hello.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/hello.elf ::hello.elf; fi
	if [ -f $(BUILD_DIR)/loop.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/loop.elf ::loop.elf; fi
	if [ -f $(BUILD_DIR)/parent.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/parent.elf ::parent.elf; fi
	if [ -f $(BUILD_DIR)/child.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/child.elf ::child.elf; fi
	if [ -f $(BUILD_DIR)/hello_c.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/hello_c.elf ::hello_c.elf; fi
	if [ -f $(BUILD_DIR)/iotest.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/iotest.elf ::iotest.elf; fi
	if [ -f $(BUILD_DIR)/keytest.elf ]; then mcopy -i $(BUILD_DIR)/disk.img $(BUILD_DIR)/keytest.elf ::keytest.elf; fi
	if [ -f doom/doomgeneric-master/doomgeneric/doom.elf ]; then mcopy -i $(BUILD_DIR)/disk.img doom/doomgeneric-master/doomgeneric/doom.elf ::doom.elf; fi
	# Copy any WAD files found in the doom folder (case-insensitive)
	find doom -iname "*.wad" -exec mcopy -i $(BUILD_DIR)/disk.img {} :: \;


$(BUILD_DIR)/data.img: test_ext2_data/hello.txt test_ext2_data/readme.txt
	mkdir -p $(BUILD_DIR)
	rm -f $(BUILD_DIR)/data.img
	dd if=/dev/zero of=$(BUILD_DIR)/data.img bs=1M count=10
	mkfs.ext2 -F -d test_ext2_data $(BUILD_DIR)/data.img

disk_image: $(BUILD_DIR)/disk.img $(BUILD_DIR)/data.img

run: floppy_image disk_image
	qemu-system-x86_64 -fda $(BUILD_DIR)/main_floppy.img -hda $(BUILD_DIR)/disk.img -hdb $(BUILD_DIR)/data.img -smp 2 -boot order=a -serial stdio -netdev user,id=net0 -device rtl8139,netdev=net0 -vga std

run-persist: floppy_image
	qemu-system-x86_64 -fda $(BUILD_DIR)/main_floppy.img -hda $(BUILD_DIR)/disk.img -hdb $(BUILD_DIR)/data.img -smp 2 -boot order=a -serial stdio -netdev user,id=net0 -device rtl8139,netdev=net0 -vga std

run-with-user: user_programs floppy_image disk_image
	qemu-system-x86_64 -fda $(BUILD_DIR)/main_floppy.img -hda $(BUILD_DIR)/disk.img -hdb $(BUILD_DIR)/data.img -smp 2 -boot order=a -serial stdio -netdev user,id=net0 -device rtl8139,netdev=net0 -vga std

clean:
	rm -rf $(BUILD_DIR)
	cd kernel && $$HOME/.cargo/bin/cargo clean
	$(MAKE) -C libc clean
