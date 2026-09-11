.section .text
.globl _start
_start:
  li t0,1<<13
  csrs mstatus,t0
  li t0,0x8001
  fmv.h.x f1,t0
  fmv.x.h t1,f1
  li t2,-32767
  bne t1,t2,fail
  fmv.x.d t1,f1
  li t2,0xffffffffffff8001
  bne t1,t2,fail
  la t0,scratch
  fsh f1,0(t0)
  lhu t1,0(t0)
  li t2,0x8001
  bne t1,t2,fail
  flh f2,0(t0)
  fmv.x.d t1,f2
  li t2,0xffffffffffff8001
  bne t1,t2,fail
  # An improperly boxed half converts to quiet NaN without NV.
  li t0,0x7c01
  fmv.d.x f1,t0
  csrw fflags,zero
  fcvt.s.h f2,f1
  fmv.x.w t1,f2
  li t2,0x7fc00000
  bne t1,t2,fail
  csrr t1,fflags
  bnez t1,fail
  # Raw moves and stores preserve the low bits even when boxing is invalid.
  fmv.x.h t1,f1
  bne t1,t0,fail
  la t2,scratch
  fsh f1,0(t2)
  lhu t1,0(t2)
  bne t1,t0,fail
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
.section .data
.balign 8
scratch: .dword 0
