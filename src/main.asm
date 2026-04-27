org 0x7C00
bits 16


%define ENDL 0x0D, 0x0A


;
; FAT12 header
; 
jmp short start
nop

bdb_oem_name:                   db "MSWIN4.1"           ; 8 bytes
bdb_bytes_per_sector:           dw 512
bdb_sectors_per_cluster:        db 1
bdb_reserved_sectors:           dw 1
bdb_fat_count:                  db 2
bdb_dir_entries_count:          dw 224
bdb_total_sectors:              dw 2880                 ; 2880 * 512 = 1.44MB
bdb_media_descriptor_type:      db 0xF0                 ; F0 = 3.5" floppy disk
bdb_sectors_per_fat:            dw 9
bdb_sectors_per_track:          dw 18
bdb_heads:                      dw 2
bdb_hidden_sectors:             dd 0
bdb_large_sector_count:         dd 0

; extended boot record
ebr_drive_number:               db 0                    ; 0x00 floppy, 0x80 hdd, useless
                                db 0                    ; reserved
ebr_signature:                  db 0x29
ebr_volume_id:                  db 0x12, 0x34, 0x56, 0x78; any number
ebr_volume_label:               db "MY NEW OS  "        ; 11 bytes, padded with spaces
ebr_system_id:                  db "FAT12   "           ; 8 bytes


;
; Code starts here
;

start:
    ; setup data segments
    mov ax, 0           ; can't set ds/es directly
    mov ds, ax
    mov es, ax
    
    ; setup stack
    mov ss, ax
    mov sp, 0x7C00      ; stack grows downwards from where we are loaded in memory

    ; some BIOSes might start us at 07C0:0000 instead of 0000:7C00, make sure we are in the expected location
    push es
    push word .after_far_jmp
    retf

.after_far_jmp:

    ; make sure dl contains boot drive
    mov [ebr_drive_number], dl

    ; print hello world message
    mov si, msg_loading
    call puts

    ; read drive parameters (sectors per track and head count),
    ; instead of relying on our data which may be wrong
    push es
    mov ah, 0x08
    int 0x13
    jc floppy_error
    pop es

    and cx, 0x3F                ; sectors per track are in bits 0-5 of cx
    mov [bdb_sectors_per_track], cx

    movzx dx, dh                ; dh is max head number
    add dx, 1                   ; heads = max head number + 1
    mov [bdb_heads], dx

    ; read root directory
    ; compute LBA of root directory = reserved + fats * sectors_per_fat
    mov ax, [bdb_reserved_sectors]
    mov bl, [bdb_fat_count]
    mul bl
    add ax, [bdb_sectors_per_fat] ; This was `add ax, [bdb_reserved_sectors]` in the original, but should be `add ax, [bdb_sectors_per_fat]` to get to the start of the root directory.
    push ax

    ; compute size of root directory = (32 * entries) / bytes_per_sector
    mov ax, [bdb_dir_entries_count]
    shl ax, 5                           ; ax *= 32
    xor dx, dx
    div word [bdb_bytes_per_sector]     ; number of sectors in root directory

    test dx, dx
    jz .root_dir_after
    inc ax                              ; division remainder exists, add 1 sector

.root_dir_after:
    ; read root directory
    mov cl, al                          ; cl = number of sectors to read
    pop ax                              ; ax = LBA of root directory
    mov dl, [ebr_drive_number]
    mov bx, buffer                      ; es:bx = buffer
    call disk_read

    ; search for stage2.bin
    xor bx, bx                          ; bx = index of directory entry
    mov di, buffer                      ; di = pointer to directory entry

.search_stage2:
    mov si, file_stage2_bin
    mov cx, 11                          ; filename is 11 bytes
    push di
    repe cmpsb
    pop di
    je .found_stage2

    add di, 32                          ; next directory entry
    inc bx
    cmp bx, [bdb_dir_entries_count]
    jl .search_stage2

    ; kernel not found
    jmp stage2_not_found_error

.found_stage2:
    ; di points to the start of the directory entry
    mov ax, [di + 26]                   ; ax = first cluster of stage2
    mov [stage2_cluster], ax

    ; load FAT from disk into buffer
    mov ax, [bdb_reserved_sectors]
    mov cl, [bdb_sectors_per_fat]
    mov dl, [ebr_drive_number]
    mov bx, buffer
    call disk_read

    ; read stage2 and jump to it
    mov bx, STAGE2_LOAD_SEGMENT
    mov es, bx
    mov bx, STAGE2_LOAD_OFFSET          ; es:bx = memory location to load stage2

.load_stage2_loop:
    ; read cluster
    mov ax, [stage2_cluster]
    
    ; not a very good way to do this, but for now it's okay:
    ; hardcoded LBA of data region = reserved + fats * sectors_per_fat + root_dir_size
    add ax, 31                          ; ax = LBA of cluster (for 1.44MB floppy)

    mov cl, 1
    mov dl, [ebr_drive_number]
    call disk_read

    add bx, [bdb_bytes_per_sector]

    ; compute next cluster
    mov ax, [stage2_cluster]
    mov cx, 3
    mul cx
    mov cx, 2
    div cx                              ; ax = index of FAT entry, dx = remainder (0 or 1)

    mov si, buffer
    add si, ax
    mov ax, [si]

    test dx, dx
    jz .even

.odd:
    shr ax, 4                           ; ax >>= 4
    jmp .next_cluster_after

.even:
    and ax, 0x0FFF                      ; ax &= 0x0FFF

.next_cluster_after:
    cmp ax, 0x0FF8                      ; end of chain?
    jae .read_finish

    mov [stage2_cluster], ax
    jmp .load_stage2_loop

.read_finish:
    ; jump to stage2
    mov dl, [ebr_drive_number]          ; pass boot drive to stage2

    mov ax, STAGE2_LOAD_SEGMENT         ; setup segments
    mov ds, ax
    mov es, ax

    jmp STAGE2_LOAD_SEGMENT:STAGE2_LOAD_OFFSET

    jmp halt                            ; should never happen

halt:
    cli                                 ; disable interrupts
    hlt
    jmp halt


;
; Error handlers
;

floppy_error:
    mov si, msg_read_failed
    call puts
    jmp wait_key_and_reboot

stage2_not_found_error:
    mov si, msg_stage2_not_found
    call puts
    jmp wait_key_and_reboot

wait_key_and_reboot:
    mov ah, 0
    int 0x16                            ; wait for keypress
    jmp 0xFFFF:0                        ; jump to beginning of BIOS (reboots)

;
; Prints a string to the screen
; Params:
;   - ds:si points to string
;
puts:
    ; save registers we will modify
    push si
    push ax
    push bx

.loop:
    lodsb               ; loads next character in al
    or al, al           ; verify if next character is null?
    jz .done

    mov ah, 0x0E        ; call bios interrupt
    mov bh, 0           ; set page number to 0
    int 0x10

    jmp .loop

.done:
    pop bx
    pop ax
    pop si    
    ret

;
; Disk routines
;

;
; Converts an LBA address to a CHS address
; Parameters:
;   - ax: LBA address
; Returns:
;   - cx [bits 0-5]: sector number
;   - cx [bits 6-15]: cylinder
;   - dh: head
;
lba_to_chs:
    push ax
    push dx

    xor dx, dx                          ; dx = 0
    div word [bdb_sectors_per_track]    ; ax = LBA / sectors_per_track
                                        ; dx = LBA % sectors_per_track

    inc dx                              ; dx = (LBA % sectors_per_track) + 1 = sector
    mov cx, dx                          ; cx = sector

    xor dx, dx                          ; dx = 0
    div word [bdb_heads]                ; ax = (LBA / sectors_per_track) / heads = cylinder
                                        ; dx = (LBA / sectors_per_track) % heads = head
    mov dh, dl                          ; dh = head
    mov ch, al                          ; ch = cylinder (lower 8 bits)
    shl ah, 6
    or cl, ah                           ; put upper 2 bits of cylinder in cl

    pop ax
    mov dl, al                          ; restore dl
    pop ax
    ret


;
; Reads sectors from a disk
; Parameters:
;   - ax: LBA address
;   - cl: number of sectors to read (up to 128)
;   - dl: drive number
;   - es:bx: memory address where to store read data
;
disk_read:
    push ax                             ; save registers
    push bx
    push cx
    push dx
    push di

    push cx                             ; temporarily save cl (number of sectors to read)
    call lba_to_chs                     ; compute CHS
    pop ax                              ; al = number of sectors to read
    
    mov ah, 0x02
    mov di, 3                           ; retry count

.retry:
    pusha                               ; save all registers, we don't know what bios modifies
    stc                                 ; set carry flag, some BIOS don't set it on error
    int 0x13                            ; carry flag cleared = success
    jnc .done                           ; jump if success

    ; read failed
    popa
    call disk_reset

    dec di
    test di, di
    jnz .retry

.fail:
    ; all attempts failed
    jmp floppy_error

.done:
    popa

    pop di
    pop dx
    pop cx
    pop bx
    pop ax                             ; restore registers
    ret


;
; Resets disk controller
; Parameters:
;   dl: drive number
;
disk_reset:
    pusha
    mov ah, 0
    stc
    int 0x13
    jc floppy_error
    popa
    ret


STAGE2_LOAD_SEGMENT     equ 0x0000
STAGE2_LOAD_OFFSET      equ 0x0500


msg_loading:            db "Loading...", ENDL, 0
msg_read_failed:        db "Read from disk failed!", ENDL, 0
msg_stage2_not_found:   db "STAGE2.BIN file not found!", ENDL, 0
file_stage2_bin:        db "STAGE2  BIN"
stage2_cluster:         dw 0

times 510-($-$$) db 0
dw 0AA55h

buffer: