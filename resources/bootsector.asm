; fluxfox boot sector.
; This boot sector code appears in many places.
; Modified versions are written by utilities such as WinImage and UltraISO.
; The original code was written by Christoph H. Hochstaetter and included 
; in the distribution of his FDFORMAT utility. 
;
; This sector cannot just be written to the disk - after writing it a
; proper BPB (BIOS Parameter Block) must be written to the disk as well
; or the disk will be unreadable by DOS.
;
; Converted to NASM format and modified for fluxfox by Daniel Balsom
; (C)1991 Christoph H. Hochstaetter

[org 0x100]                     ; Allow Boot-Sector as COM-File

section .text

begin:
    jmp     short start         ; Skip BPB
    nop

bpb:
    times   0x57 db 0           ; Reserve enough space for BPB (87 bytes)

start:
    cli                         ; Clear Interrupts while modifying stack
    xor     ax, ax              ; Zero AX
    mov     ss, ax              ; Set SS
    mov     sp, 0x7C00          ; Set SP below code
    mov     ax, 0x7B0           ; Set Segment-Registers so Offset is 100H
    push    ax                  ; Push Segment twice
    push    ax
    pop     ds                  ; Get Segment in DS
    pop     es                  ; And also in ES
    mov     si, 0x100           ; Set Source to 100H
    mov     di, 0x300           ; Set Destination to 300H
    mov     cx, 0x100           ; Set Count to 256 words (512 bytes)
    rep     movsw               ; Move Code
    mov     ax, 0x7D0           ; New Segment at 7D0h (+20H)
    push    ax                  ; Push Segment three times
    push    ax
    push    ax
    pop     ds                  ; Get new segment in DS
    pop     es                  ; And also in ES
    mov     ax, entry           ; Offset of next instruction
    push    ax                  ; Push to stack
    retf                        ; Far return to CS:IP (entry point)

entry:
    sti                         ; Start Interrupts again
    mov     si, ascii_cow       ; Move SI to cow
    call    output              ; Display to screen
    mov     si, text1           ; Move SI to text
    call    output              ; Display to screen
    mov     ax, 0x201           ; AH=2 (read sector), AL=1 (count)
    mov     cx, 1               ; CH=0 (Track), CL=1 (Sector)
    mov     dx, 0x80            ; DH=0 (Head), DL=80h (Fixed Disk C)
    xor     bx, bx              ; BX=0
    mov     es, bx		        ; Segment of Transfer buffer (0000)
    mov     bx, 0x7C00          ; Offset of Transfer buffer
    push    es
    push    bx
    int     0x13                ; Read from Hard disk
    jc      error               ; Jump if error
    cmp     word [es:0x7DFE], 0xAA55 ; Valid Boot Sector?
    jnz     error               ; No, error
    retf                        ; Continue with Boot-Sector of C:

error:
    mov     si, text2           ; Move SI to text
    call    output              ; Display to screen

loop1:
    mov     ah, 1               ; Get Status of Keyboard buffer
    int     0x16
    jz      boot_new            ; If key pressed, reboot
    xor     ah, ah              ; Flush Keyboard Buffer
    int     0x16
    jmp     loop1               ; And try again

boot_new:
    xor     ah, ah              ; Flush Keyboard buffer
    int     0x16
    xor     dx, dx              ; Zero DX
    int     0x19                ; Reboot

output:
    cld
loop_o:
    lodsb                       ; Get one character
    or      al, al              ; Is it zero?
    jnz     loop_d              ; No, continue
    ret                         ; Else return

loop_d:
    push    si                  ; Save SI
    mov     ah, 0x0E            ; Output character in AL to screen
    int     0x10
    pop     si                  ; Restore SI
    jmp     short loop_o        ; Repeat loop

ascii_cow:
    db '\|/          (__)', 10,13
    db '     `\------(oo)', 10,13
    db '       ||    (__)', 10,13
    db '       ||w--||     \|/', 10,13
    db '   \|/', 10,13, 0

text1:
    db      'Disk image created by fluxfox ', 0x20, 10, 13
    db      'Not a system disk! Booting from hard disk...', 10, 13, 0

text2:
    db      'Cannot load from hard disk!', 10, 13
    db      'Insert a system disk and press any key...', 10, 13, 0

; Boot sector signature
times   0x1FE - ($ - $$) db 0   ; Pad up to 0x1FE
dw      0xAA55                  ; Boot sector signature