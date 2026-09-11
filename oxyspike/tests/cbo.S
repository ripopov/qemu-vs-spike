.option norvc
.section .text
.globl _start
_start:
  la t0,trap
  csrw mtvec,t0
  li s5,0
  la t0,buffer
  li t1,-1
  sd t1,0(t0)
  sd t1,64(t0)
  lr.d t2,(t0)
  addi t0,t0,7
  .word 0x0042a00f # cbo.zero (t0), unaligned byte address
  addi t0,t0,-7
  ld t2,0(t0)
  bnez t2,fail
  ld t2,64(t0)
  bne t2,t1,fail
  sc.d t2,t1,(t0) # Spike preserves the reservation across cbo.zero.
  bnez t2,fail

.macro fault encoding,address
  li t0,\address
  li s5,1
  li s1,\address
  la s2,1f
1: .word \encoding
  bnez s5,fail
.endm
  # Every operation rejects MMIO and reports the original byte address.
  fault 0x0002a00f,0x2000007
  fault 0x0012a00f,0x2000007
  fault 0x0022a00f,0x2000007
  fault 0x0042a00f,0x2000007
  fault 0x0002a00f,0x10000007
  fault 0x0042a00f,0x10000007

  # A locked read-only eight-byte PMP region inside the block. Checking
  # the addressed byte permits clean/invalidate; requiring a full block fails.
  la t0,buffer
  addi t0,t0,16
  srli t0,t0,2
  csrw pmpaddr0,t0
  li t0,0x99
  csrw pmpcfg0,t0
  la t0,buffer
  addi t0,t0,17
  .word 0x0002a00f
  .word 0x0012a00f
  .word 0x0022a00f
  li s5,1
  mv s1,t0
  la s2,zero_denied
zero_denied:
  .word 0x0042a00f
  bnez s5,fail
  li a0,1
  j exit
trap:
  beqz s5,fail
  csrr t1,mcause
  li t2,7
  bne t1,t2,fail
  csrr t1,mtval
  bne t1,s1,fail
  csrr t1,mepc
  bne t1,s2,fail
  addi t1,t1,4
  csrw mepc,t1
  li s5,0
  mret
fail:
  li a0,3
exit:
  la t0,tohost
  sd a0,0(t0)
1: j 1b
.section .data
.balign 64
buffer: .zero 128
.section .tohost,"aw",@progbits
.balign 64
.globl tohost
tohost: .dword 0
.balign 64
.globl fromhost
fromhost: .dword 0
