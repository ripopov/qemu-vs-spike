.option norvc
.section .text
.globl _start
_start:
  la t0,trap_handler
  csrw mtvec,t0
  li t0,-1
  csrw pmpaddr0,t0
  li t0,31
  csrw pmpcfg0,t0
  la s0,root
  srli t0,s0,12
  li t1,8<<60
  or t0,t0,t1
  csrw satp,t0
  la s1,value
  li s3,0
loop:
  # Test all PBMT encodings with PBMTE enabled.
  li t0,1<<62
  csrw menvcfg,t0
  li t0,0x200000cf
  slli t1,s3,61
  or t0,t0,t1
  sd t0,16(s0)
  sfence.w.inval
  sinval.vma zero,zero
  sfence.inval.ir
  li s2,0
  li t0,(1<<17)|(1<<11)
  csrw mstatus,t0
  ld t2,0(s1)
  csrw mstatus,zero
  li t0,3
  beq s3,t0,expect_fault
  bnez s2,fail
  li t0,42
  bne t2,t0,fail
  j next
expect_fault:
  li t0,1
  bne s2,t0,fail
next:
  addi s3,s3,1
  li t0,4
  bne s3,t0,loop
  # Nonzero PBMT must fault when PBMTE is disabled.
  li t0,(1<<61)|0x200000cf
  sd t0,16(s0)
  csrw menvcfg,zero
  sfence.vma
  li s2,0
  li t0,(1<<17)|(1<<11)
  csrw mstatus,t0
  ld t2,0(s1)
  csrw mstatus,zero
  li t0,1
  bne s2,t0,fail
  li a0,1
  j exit
.balign 4
trap_handler:
  csrr t0,mcause
  li t1,13
  bne t0,t1,fail
  addi s2,s2,1
  csrr t0,mepc
  addi t0,t0,4
  csrw mepc,t0
  li t0,1<<17
  csrc mstatus,t0
  mret
fail:
  li a0,3
exit:
  csrw mstatus,zero
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
value: .dword 42
.section .bss
.balign 4096
root: .space 4096
