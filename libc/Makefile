# Makefile for MYNEWOS libc

CC = x86_64-linux-gnu-gcc
AS = nasm
AR = x86_64-linux-gnu-ar

CFLAGS = -Wall -Wextra -ffreestanding -nostdlib -Iinclude -mcmodel=large -fno-stack-protector -mno-red-zone
ASFLAGS = -f elf64

SRCS = src/malloc.c src/string.c src/stdio.c src/stdlib.c src/math.c
ASMS = src/syscall.S

OBJS = $(SRCS:.c=.o) $(ASMS:.S=.o)

all: libc.a src/crt0.o

libc.a: $(OBJS)
	$(AR) rcs $@ $(OBJS)

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

%.o: %.S
	$(AS) $(ASFLAGS) $< -o $@

clean:
	rm -f $(OBJS) libc.a crt0.o
