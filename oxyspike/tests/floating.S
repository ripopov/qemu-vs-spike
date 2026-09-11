.section .text
.globl _start
_start:
  li t0,1<<13
  csrs mstatus,t0
  la s0,cases
  la s1,cases_end
loop:
  ld a0,0(s0)
  ld a1,8(s0)
  ld a2,16(s0)
  fmv.d.x f1,a0
  fmv.d.x f2,a1
  fmv.d.x f3,a2
  lw t0,40(s0)
  la t1,slot
  sw t0,0(t1)
  fence.i
  csrw fflags,zero
slot:
  .word 0x00000013
  lbu t0,44(s0)
  li t1,1
  beq t0,t1,integer_result
  fmv.x.d t3,f28
  beqz t0,integer_result
  slli t3,t3,32
  srli t3,t3,32
integer_result:
  ld t0,24(s0)
  bne t3,t0,fail
  csrr t0,fflags
  ld t1,32(s0)
  bne t0,t1,fail
  addi s0,s0,48
  bne s0,s1,loop
  li a0,1
  j exit
fail:
  li a0,3
exit:
  la t0,tohost
  sd a0,0(t0)
1: j 1b
.section .tohost,"aw",@progbits
.balign 64
.globl tohost
tohost: .dword 0
.balign 64
.globl fromhost
fromhost: .dword 0
.section .rodata
.balign 8
cases:
.incbin "build/oxyspike-tests/floating-cases.bin"
cases_end:
