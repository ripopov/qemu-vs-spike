.section .text
.globl _start
_start:
  li t0,5
  csrw mcountinhibit,t0
  li t1,100
  csrw minstret,t1
  csrw mcycle,t1
  csrr t2,minstret
  bne t2,t1,fail
  csrr t2,mcycle
  bne t2,t1,fail
  csrw mcountinhibit,zero
  csrr t2,minstret
  csrr t3,minstret
  sub t3,t3,t2
  li t0,1
  bne t3,t0,fail
  csrw minstret,zero
  csrr t2,minstret
  bnez t2,fail
  csrw mcycle,zero
  csrr t2,mcycle
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
