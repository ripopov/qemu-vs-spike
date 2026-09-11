OUTPUT_ARCH(riscv)
ENTRY(_start)
SECTIONS {
  . = 0x80000000;
  .text : { *(.text .text.*) }
  . = ALIGN(0x1000);
  .rodata : { *(.rodata .rodata.*) }
  .data : { *(.data .data.*) }
  .tohost : { *(.tohost) }
  .bss : { *(.bss .bss.*) *(COMMON) }
}
