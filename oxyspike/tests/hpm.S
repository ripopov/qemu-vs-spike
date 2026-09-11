.option norvc
.section .text
.globl _start
_start:
  la t0,trap
  csrw mtvec,t0
  li s5,0
  li t1,-1
  csrw pmpaddr0,t1
  li t0,31
  csrw pmpcfg0,t0
.irp n,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
  csrw (0xb00+\n),t1
  csrr t0,(0xb00+\n)
  bnez t0,fail
  csrw (0x320+\n),t1
  csrr t0,(0x320+\n)
  bnez t0,fail
.endr
  csrw mcounteren,t1
  csrw scounteren,zero
  li t0,0x800
  csrw mstatus,t0
  la t0,supervisor
  csrw mepc,t0
  la s0,after_supervisor
  mret
supervisor:
.irp n,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
  csrr t0,(0xc00+\n)
  bnez t0,fail
.endr
  ecall
after_supervisor:
  csrw mstatus,zero
  la t0,user_denied
  csrw mepc,t0
  la s0,after_denied
  mret
user_denied:
.irp n,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
  li s5,1
  csrr t0,(0xc00+\n)
  bnez s5,fail
.endr
  ecall
after_denied:
  li t1,-1
  csrw scounteren,t1
  csrw mstatus,zero
  la t0,user_allowed
  csrw mepc,t0
  la s0,pass
  mret
user_allowed:
.irp n,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
  csrr t0,(0xc00+\n)
  bnez t0,fail
  li s5,1
  csrw (0xc00+\n),zero
  bnez s5,fail
.endr
  ecall
trap:
  csrr t0,mcause
  li t1,2
  beq t0,t1,illegal
  li t1,8
  beq t0,t1,returned
  li t1,9
  bne t0,t1,fail
returned:
  csrw mepc,s0
  li t0,0x1800
  csrs mstatus,t0
  mret
illegal:
  beqz s5,fail
  li s5,0
  csrr t0,mepc
  addi t0,t0,4
  csrw mepc,t0
  mret
pass:
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
