; =============================================================================
; stage2.asm - Loads KERNEL.BIN, switches to 64-bit Long Mode, jumps to Rust
; =============================================================================

bits 16
org 0x0500

; Memory Layout Constants
KERNEL_LOAD_SEG     equ 0x1000      ; Segment to load kernel (abs 0x10000)
KERNEL_DEST         equ 0x100000    ; 1MB - where kernel is linked
PAGE_TABLE_BASE     equ 0x70000     ; Page tables in safe upper memory

start:
    ; Save boot drive
    mov [boot_drive], dl

    ; Setup segments
    xor ax, ax
    mov ds, ax
    mov es, ax

    ; Print stage2 banner
    mov si, msg_stage2
    call puts

    ; -------------------------------------------------------------------------
    ; Enable A20 Gate (fast method)
    ; -------------------------------------------------------------------------
    in al, 0x92
    or al, 2
    out 0x92, al

    ; -------------------------------------------------------------------------
    ; Query System Memory Map (INT 15h, AX=E820)
    ; -------------------------------------------------------------------------
    mov di, 0x9000          ; Destination buffer for memory map
    xor ebx, ebx            ; Continuation counter (must be 0 at start)
    mov edx, 0x534D4150     ; 'SMAP' signature
    mov eax, 0xE820
    mov [di + 20], dword 1  ; Force ACPI 3.x entry to 1 for safety
    mov ecx, 24             ; Buffer size
    int 0x15
    jc .e820_fail
    mov edx, 0x534D4150     ; Some BIOSes corrupt EDX
    cmp eax, edx
    jne .e820_fail
    test ebx, ebx           ; If ebx=0, list might be 1 long
    jz .e820_done

.e820_loop:
    inc word [mmap_entries]
    add di, 24              ; Increment buffer address
    mov [di + 20], dword 1  ; Force ACPI
    mov ecx, 24
    mov edx, 0x534D4150
    mov eax, 0xE820
    int 0x15
    jc .e820_done
    test ebx, ebx
    jnz .e820_loop

.e820_done:
    inc word [mmap_entries] ; count last entry
    jmp .e820_finish

.e820_fail:
    mov si, msg_e820_err
    call puts
    cli
    hlt

.e820_finish:

    ; -------------------------------------------------------------------------
    ; Step 1: Read Root Directory from disk into buffer
    ;   Root Dir LBA = Reserved(1) + FATs(2)*SectorsPerFAT(9) = 19
    ;   Root Dir Size = ceil(224*32 / 512) = 14 sectors
    ; -------------------------------------------------------------------------
    mov ax, 19
    mov cl, 14
    xor bx, bx
    mov bx, buffer
    call disk_read

    ; -------------------------------------------------------------------------
    ; Step 2: Search root directory for "KERNEL  BIN" (FAT 8.3 format)
    ; -------------------------------------------------------------------------
    mov di, buffer
    mov cx, 224
.search:
    push cx
    push di
    mov si, file_kernel_bin
    mov cx, 11
    repe cmpsb
    pop di
    pop cx
    je .found
    add di, 32
    loop .search

    ; Not found - print error and halt
    mov si, msg_not_found
    call puts
    cli
    hlt

.found:
    ; Save first cluster number (offset 26 in dir entry)
    mov ax, [di + 26]
    mov [kernel_cluster], ax

    mov si, msg_kernel_found
    call puts

    ; -------------------------------------------------------------------------
    ; Step 3: Load FAT table into buffer
    ; -------------------------------------------------------------------------
    mov ax, 1
    mov cl, 9
    mov bx, buffer
    call disk_read

    ; -------------------------------------------------------------------------
    ; Step 4: Load kernel clusters into KERNEL_LOAD_SEG:0x0000
    ; -------------------------------------------------------------------------
    mov ax, KERNEL_LOAD_SEG
    mov es, ax
    xor bx, bx                     ; es:bx = 0x1000:0x0000

.load_loop:
    ; Convert cluster to LBA: LBA = cluster - 2 + 33
    mov ax, [kernel_cluster]
    add ax, 31                      ; data_start(33) - 2 = 31

    mov cl, 1
    call disk_read
    
    add bx, 512
    jnz .no_es_wrap
    mov ax, es
    add ax, 0x1000
    mov es, ax
.no_es_wrap:

    ; Follow FAT12 chain to get next cluster
    mov ax, [kernel_cluster]
    mov cx, 3
    mul cx
    mov cx, 2
    div cx                          ; ax = byte offset, dx = 0(even) or 1(odd)

    push ds
    xor cx, cx
    mov ds, cx                      ; ensure DS = 0 for buffer access
    mov si, buffer
    add si, ax
    mov ax, [ds:si]
    pop ds

    test dx, dx
    jz .even
.odd:
    shr ax, 4
    jmp .next
.even:
    and ax, 0x0FFF
.next:
    cmp ax, 0x0FF8
    jae .load_done
    mov [kernel_cluster], ax
    jmp .load_loop

.load_done:
    ; Reset ES back to 0
    xor ax, ax
    mov es, ax

    mov si, msg_kernel_loaded
    call puts

    mov si, msg_switching
    call puts

    ; -------------------------------------------------------------------------
    ; Switch to 32-bit Protected Mode
    ; -------------------------------------------------------------------------
    cli

    ; Disable NMI
    in al, 0x70
    or al, 0x80
    out 0x70, al

    ; Load dummy IDT (to prevent triple faults if an interrupt fires)
    lidt [idt_dummy]

    ; Load 32-bit GDT
    lgdt [gdt32_desc]

    ; Enable Protected Mode
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    ; Far jump to flush instruction pipeline and set CS
    jmp 0x08:enter_pm

; =============================================================================
; 16-bit Helper Functions
; =============================================================================

; puts: Print null-terminated string at DS:SI via BIOS
puts:
    push si
    push ax
.loop:
    lodsb
    or al, al
    jz .done
    mov ah, 0x0E
    mov bh, 0
    int 0x10
    jmp .loop
.done:
    pop ax
    pop si
    ret

; disk_read: Read sectors from floppy
;   AX = LBA, CL = count, ES:BX = dest buffer
disk_read:
    pusha
    mov di, bx                  ; save buffer offset in DI

    ; Save sector count
    push cx

    ; LBA to CHS for 1.44MB floppy (18 spt, 2 heads)
    xor dx, dx
    mov bx, 18
    div bx                      ; ax = LBA/18, dx = LBA%18
    inc dx
    mov cl, dl                  ; CL = sector (1-based)

    xor dx, dx
    mov bx, 2
    div bx                      ; ax = cylinder, dx = head
    mov ch, al                  ; CH = cylinder
    mov dh, dl                  ; DH = head

    mov dl, [boot_drive]        ; DL = drive number
    mov bx, di                  ; BX = buffer offset (restored!)

    pop ax                      ; AL = sector count
    mov ah, 0x02                ; BIOS read sectors

    mov di, 3                   ; retry count
.retry:
    pusha
    stc
    int 0x13
    jnc .ok
    popa
    dec di
    jnz .retry

    ; All retries failed
    mov si, msg_disk_err
    call puts
    cli
    hlt

.ok:
    popa
    popa                        ; restore original registers
    ret

; =============================================================================
; 16-bit Data
; =============================================================================
boot_drive:       db 0
kernel_cluster:   dw 0
file_kernel_bin:  db "KERNEL  BIN"        ; FAT12 8.3 format (8+3, space padded)

msg_stage2:       db "Stage 2 loaded.", 0x0D, 0x0A, 0
msg_kernel_found: db "Found KERNEL.BIN", 0x0D, 0x0A, 0
msg_kernel_loaded:db "Kernel loaded to memory.", 0x0D, 0x0A, 0
msg_switching:    db "Switching to 64-bit mode...", 0x0D, 0x0A, 0
msg_not_found:    db "KERNEL.BIN not found!", 0x0D, 0x0A, 0
msg_disk_err:     db "Disk read error!", 0x0D, 0x0A, 0
msg_e820_err:     db "E820 Mem Map failed!", 0x0D, 0x0A, 0
mmap_entries:     dw 0

; =============================================================================
; 32-bit Protected Mode Code
; =============================================================================
bits 32

enter_pm:
    ; Setup 32-bit data segments
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000

    ; -------------------------------------------------------------------------
    ; Copy kernel from 0x10000 to 0x100000 (1MB)
    ; -------------------------------------------------------------------------
    cld
    mov esi, 0x10000
    mov edi, KERNEL_DEST
    mov ecx, 65536              ; 256KB / 4 = 65536 dwords
    rep movsd

    ; -------------------------------------------------------------------------
    ; Setup paging for 64-bit (identity map first 2MB)
    ; Page tables at PAGE_TABLE_BASE (0x70000)
    ;   PML4 at 0x70000, PDP at 0x71000, PD at 0x72000
    ; -------------------------------------------------------------------------

    ; Zero out 3 pages (12KB) of page table memory
    mov edi, PAGE_TABLE_BASE
    xor eax, eax
    mov ecx, 3072               ; 12288 bytes / 4
    rep stosd

    ; PML4[0] -> PDP
    mov dword [PAGE_TABLE_BASE], (PAGE_TABLE_BASE + 0x1000) | 3

    ; PDP[0] -> PD
    mov dword [PAGE_TABLE_BASE + 0x1000], (PAGE_TABLE_BASE + 0x2000) | 3

    ; Map 64 2MB huge pages (128MB identity map)
    mov edi, PAGE_TABLE_BASE + 0x2000
    xor eax, eax
    or eax, 0x83                        ; Present + Writable + HugePage (0x80)
    mov ecx, 64
.map_loop:
    mov dword [edi], eax
    mov dword [edi + 4], 0              ; Clear high dword
    add edi, 8
    add eax, 0x200000                   ; next 2MB address
    loop .map_loop

    ; PML4[511] -> PML4 address (Recursive Mapping for Page Table editing)
    mov dword [PAGE_TABLE_BASE + 511 * 8], PAGE_TABLE_BASE | 3

    ; -------------------------------------------------------------------------
    ; Enable Long Mode
    ; -------------------------------------------------------------------------

    ; Load PML4 into CR3
    mov eax, PAGE_TABLE_BASE
    mov cr3, eax

    ; Enable PAE in CR4
    mov eax, cr4
    or eax, (1 << 5)
    mov cr4, eax

    ; Set LME bit in EFER MSR
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8)
    wrmsr

    ; Enable paging in CR0
    mov eax, cr0
    or eax, (1 << 31)
    mov cr0, eax

    ; Load 64-bit GDT and far jump to long mode
    lgdt [gdt64_desc]
    jmp 0x08:enter_lm

; =============================================================================
; 64-bit Long Mode Code
; =============================================================================
bits 64

enter_lm:
    ; Setup 64-bit data segments
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov rsp, 0x90000
    sub rsp, 8

    ; Pass arguments to Rust kernel_main (System V ABI: RDI, RSI)
    mov rdi, 0x9000         ; Param 1: mmap_ptr
    xor rsi, rsi
    mov si, [mmap_entries] ; Param 2: mmap_cnt

    ; Jump to Rust kernel at 1MB
    mov rax, KERNEL_DEST
    jmp rax

    ; Should never reach here
.halt:
    cli
    hlt
    jmp .halt

; =============================================================================
; GDTs
; =============================================================================

; --- 32-bit GDT (for PM transition) ---
gdt32_start:
    dq 0                                                ; Null descriptor
    db 0xFF, 0xFF, 0, 0, 0, 10011010b, 11001111b, 0    ; 0x08: Code32
    db 0xFF, 0xFF, 0, 0, 0, 10010010b, 11001111b, 0    ; 0x10: Data32
gdt32_end:

gdt32_desc:
    dw gdt32_end - gdt32_start - 1
    dd gdt32_start

; --- 64-bit GDT (for LM transition) ---
gdt64_start:
    dq 0                                                ; 0x00: Null
    db 0, 0, 0, 0, 0, 10011010b, 00100000b, 0          ; 0x08: Code64
    db 0, 0, 0, 0, 0, 10010010b, 00000000b, 0          ; 0x10: Data64
gdt64_end:

gdt64_desc:
    dw gdt64_end - gdt64_start - 1
    dd gdt64_start

; --- Dummy IDT (to prevent triple faults) ---
align 4
idt_dummy:
    dw 0        ; Limit = 0
    dd 0        ; Base = 0

; =============================================================================
; Buffer (must be last - used for FAT/directory reads)
; =============================================================================
align 16
buffer:
