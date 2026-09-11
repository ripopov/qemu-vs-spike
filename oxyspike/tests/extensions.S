.section .text
.globl _start
_start:
  li t0,1<<13
  csrs mstatus,t0
  li t0,-123
  fmv.d.x f0,t0
  la t1,scratch
  fsd f0,0(t1)
  fld f1,0(t1)
  fmv.x.d t2,f1
  bne t0,t2,fail
  fmv.w.x f0,t0
  fmv.x.w t2,f0
  bne t0,t2,fail
  li t0,0x67
  csrw fcsr,t0
  csrr t2,fcsr
  bne t0,t2,fail
  li t0,0x12345678
  li t1,9
  sh3add t2,t0,t1
  li t3,0x91a2b3c9
  bne t2,t3,fail
  rev8 t2,t0
  li t3,0x7856341200000000
  bne t2,t3,fail
  li t0,-1
  clz t2,t0
  bnez t2,fail
  cpop t2,t0
  li t3,64
  bne t2,t3,fail
  li t0,-1
  slli.uw t2,t0,4
  li t3,0xffffffff0
  bne t2,t3,fail
  li t0,0
  li t1,63
  bset t2,t0,t1
  li t3,0x8000000000000000
  bne t2,t3,fail
  rori t2,t2,63
  li t3,1
  bne t2,t3,fail
  li t0,-1
  li t1,9
  sh1add.uw t2,t0,t1
  li t3,0x200000007
  bne t2,t3,fail
  zext.h t2,t0
  li t3,65535
  bne t2,t3,fail
  sh2add.uw t0,t0,t1
  li t3,0x400000005
  bne t0,t3,fail
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
.section .bss
.balign 8
scratch: .space 8
