.option norvc
.section .text
.globl _start
_start:
  la t0,trap
  csrw mtvec,t0
.macro check encoding,address,cause,value
  li t0,\address
  li s1,\cause
  li s2,\value
  la s3,1f
  la s4,2f
1: .word \encoding
  j fail
2:
.endm
  # Reserved AMO funct5 and LR with nonzero rs2 must fail decode first.
  check 0x3002a32f,1,2,0x3002a32f
  check 0x3002a32f,0,2,0x3002a32f
  check 0x1012a32f,1,2,0x1012a32f
  check 0x1012a32f,0,2,0x1012a32f
  # CLINT is readable/writable, but not reservable RAM.
  check 0x1002a32f,0x2000000,5,0x2000000
  check 0x1802a32f,0x2000000,7,0x2000000
  check 0x1002b32f,0x2004000,5,0x2004000
  check 0x1802b32f,0x2004000,7,0x2004000
  li a0,1
  j exit
.balign 4
trap:
  csrr t0,mcause
  bne t0,s1,fail
  csrr t0,mtval
  bne t0,s2,fail
  csrr t0,mepc
  bne t0,s3,fail
  csrw mepc,s4
  mret
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
