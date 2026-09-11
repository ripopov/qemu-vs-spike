.option norvc
.option nopic
.section .text
.globl _start
_start:
  la t0,trap
  csrw mtvec,t0
  li t0,-1
  csrw pmpaddr0,t0
  li t0,0x1f
  csrw pmpcfg0,t0
  la t0,level1
  srli t0,t0,12
  slli t0,t0,10
  ori t0,t0,1
  la t1,root
  sd t0,0(t1)
  la t0,level0
  srli t0,t0,12
  slli t0,t0,10
  ori t0,t0,1
  la t1,level1
  sd t0,0(t1)
  la t0,target
  srli t0,t0,12
  slli t0,t0,10
  ori t0,t0,0xcf
  la t1,level0
  sd t0,32(t1)
  la t0,root
  srli t0,t0,12
  li t1,8<<60
  or t0,t0,t1
  csrw satp,t0
  li t0,(1<<17)|(1<<11)
  csrw mstatus,t0
  li t0,0x4ffd
  li t1,0x1122334455667788
  la s0,faulting_store
faulting_store:
  sd t1,0(t0)
  j fail
.balign 4
trap:
  csrr t0,mcause
  li t1,15
  bne t0,t1,fail
  csrr t0,mtval
  li t1,0x5000
  bne t0,t1,fail
  csrr t0,mepc
  bne t0,s0,fail
  # Restore ordinary machine accesses before checking partial writes.
  li t0,1<<17
  csrc mstatus,t0
  la t0,target
  li t1,4093
  add t0,t0,t1
  lbu t1,0(t0)
  li t2,0x88
  bne t1,t2,fail
  lbu t1,1(t0)
  li t2,0x77
  bne t1,t2,fail
  lbu t1,2(t0)
  li t2,0x66
  bne t1,t2,fail
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
.balign 4096
root: .space 4096
level1: .space 4096
level0: .space 4096
target: .space 4096
