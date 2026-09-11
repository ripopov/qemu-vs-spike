.section .text
.globl _start
_start:
  # Spike resets mtimecmp to zero, initially asserting MTIP.
  li t0,0x2004000
  ld t1,0(t0)
  bnez t1,fail
  csrr t1,mip
  andi t1,t1,128
  beqz t1,fail
  li t0,0x2000000
  li t1,1
  sb t1,0(t0)
  sb zero,1(t0)
  sb zero,2(t0)
  sb zero,3(t0)
  ld t2,0(t0)
  bne t2,t1,fail
  csrr t2,mip
  andi t2,t2,8
  beqz t2,fail
  li t1,0xffffffff00000000
  sd t1,0(t0)
  ld t2,0(t0)
  bnez t2,fail
  csrr t2,mip
  andi t2,t2,8
  bnez t2,fail
  li t1,-1
  sd t1,8(t0)
  ld t2,8(t0)
  bnez t2,fail
  li t0,0x2004000
  li t1,0x0123456789abcdef
  sd t1,0(t0)
  li t1,0xdeadbeef
  sw t1,4(t0)
  li t1,0x12
  sb t1,1(t0)
  ld t2,0(t0)
  li t1,0xdeadbeef89ab12ef
  bne t2,t1,fail
  sd t1,8(t0)
  ld t2,8(t0)
  bnez t2,fail
  li t0,0x200bff0
  sd t1,0(t0)
  ld t2,0(t0)
  bnez t2,fail
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
