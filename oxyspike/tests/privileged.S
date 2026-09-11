.section .text
.globl _start
_start:
  la t0, machine_trap
  csrw mtvec,t0
  li t0,-1
  csrw pmpaddr0,t0
  li t0,0x1f
  csrw pmpcfg0,t0
  li t0,1<<8
  csrw medeleg,t0
  la t0, supervisor_trap
  csrw stvec,t0
  li t0,(1<<11)|(1<<13)
  csrw mstatus,t0
  la t0, supervisor
  csrw mepc,t0
  mret
supervisor:
  li s0,0
  ecall
  li t0,1
  bne s0,t0,fail
  la t0,user
  csrw sepc,t0
  li t0,1<<8
  csrc sstatus,t0
  sret
user:
  ecall
  j fail
.balign 4
supervisor_trap:
  csrr t0,scause
  li t1,8
  bne t0,t1,fail
  li a0,1
  j exit
.balign 4
machine_trap:
  csrr t0,mcause
  li t1,9
  bne t0,t1,fail
  addi s0,s0,1
  csrr t0,mepc
  addi t0,t0,4
  csrw mepc,t0
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
