.option norvc
.section .text
.globl _start
_start:
  la t0,trap
  csrw mtvec,t0
  la t0,deny48
  srli t0,t0,2
  csrw pmpaddr0,t0
  la t0,deny64
  srli t0,t0,2
  csrw pmpaddr1,t0
  la t0,deny64six
  srli t0,t0,2
  csrw pmpaddr2,t0
  li t0,0x909090
  csrw pmpcfg0,t0
.macro check address,cause,value
  la s3,\address
  li s1,\cause
  \value
  la s4,1f
  jr s3
1:
.endm
  check long48,2,"li s2,0xdead001f"
  check long64,2,"li s2,0xbeef003f"
  check reserved,2,"li s2,0xcafe007f"
  check edge48,1,"la s2,deny48"
  check edge64,1,"la s2,deny64"
  check edge64six,1,"la s2,deny64six"
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
.balign 4
long48: .word 0xdead001f
  .2byte 0xabcd
long64: .word 0xbeef003f
  .word 0x87654321
reserved: .word 0xcafe007f
  .word 0x12345678
.balign 4096
.space 4092
edge48: .word 0xdead001f
deny48: .word 0
.balign 4096
.space 4092
edge64: .word 0xbeef003f
deny64: .word 0
.balign 4096
.space 4090
edge64six: .2byte 0x003f
  .word 0x12345678
deny64six: .word 0
.section .tohost,"aw",@progbits
.balign 64
.globl tohost
tohost: .dword 0
.balign 64
.globl fromhost
fromhost: .dword 0
