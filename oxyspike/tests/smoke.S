.section .text
.globl _start
_start:
  li s0, 0
  li s1, 100
1:
  add s0, s0, s1
  addi s1, s1, -1
  bnez s1, 1b
  li t0, 5050
  bne s0, t0, fail
  la a0, scratch
  sd s0, 0(a0)
  ld a1, 0(a0)
  bne a1, t0, fail
  li a2, -1
  div a3, a2, zero
  bne a3, a2, fail
  rem a3, a2, zero
  bne a3, a2, fail
  li a4, 7
  mul a5, a4, a4
  li t0, 49
  bne a5, t0, fail
  lr.d a1, (a0)
  sc.d a1, a4, (a0)
  bnez a1, fail
  ld a1, (a0)
  bne a1, a4, fail
  li a0, 1
  j exit
fail:
  li a0, 3
exit:
  la t0, tohost
  sd a0, 0(t0)
2: j 2b
.section .tohost,"aw",@progbits
.balign 64
.globl tohost
tohost: .dword 0
.balign 64
.globl fromhost
fromhost: .dword 0
.section .bss
.balign 8
scratch: .space 8
