; SPDX-License-Identifier: MIT
; Original fontpixel 8086 CPX loader. Assemble: nasm -f bin stub.asm -o stub.bin
; No UPX/UCL assembly is included. The first eight bytes are the public
; FreeDOS MODE consumer's required probe/patch interface.
; The bootstrap immediates marked PATCH are filled by the native writer.
; DS=ES=CS on entry. SS:SP belongs to the caller and must be preserved.
; The decoder accepts only this writer's valid NRV2B subset: literals,
; distance 1..256 (or repeated distance), and the standard end marker.
bits 16
cpu 8086
org 0x100

bootstrap:
    cmp sp, strict word 0          ; PATCH at 2: required stack limit
    ja short relocate             ; MODE changes this opcode to JMP
    int 0x20
relocate:
    mov cx, 0                     ; PATCH at 9: stream + decoder length
    mov si, 0                     ; PATCH at 12: original last byte
    mov di, 0                     ; PATCH at 15: relocated last byte
    std
    rep movsb
    cld
    mov si, 0                     ; PATCH at 22: relocated stream start
    mov di, 0x100
    xor bx, bx                    ; buffered control word
    xor dx, dx                    ; bits remaining in that word
    mov bp, 1                     ; initial previous match distance
    mov ax, 0                     ; PATCH at 35: relocated decoder entry
    jmp ax

; Production inserts its UPX pack header and compressed bytes here.
decoder:
next_token:
    call bit
    jnc match
    movsb
    jmp short next_token
match:
    call bit
    mov cx, 2
    adc cx, 0                     ; short number code: 2 or 3
    call bit
    jnc done                      ; only the standard end code is long here
    cmp cx, 2
    je length
    xor ax, ax
    lodsb
    mov bp, ax
    inc bp
length:
    call bit
    mov cx, 0
    adc cx, cx
    call bit
    adc cx, cx
    jcxz long_length
    inc cx
    jmp short copy_match
long_length:
    mov cx, 1
length_pair:
    call bit
    adc cx, cx
    call bit
    jnc length_pair
    add cx, 3
copy_match:
    push si
    mov si, di
    sub si, bp
    rep movsb
    pop si
    jmp short next_token
done:
    mov ax, 0x100
    jmp ax                        ; executes MODE's patched RETF
bit:
    test dx, dx
    jnz have_word
    lodsw
    mov bx, ax
    mov dx, 16
have_word:
    shl bx, 1
    dec dx                        ; DEC preserves the returned carry bit
    ret
