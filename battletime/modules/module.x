/* A module's one block of RAM: its code, then read-only data, data and zeroed data, linked at 0. */
ENTRY(module_entry)
SECTIONS {
  . = 0;
  .text : { KEEP(*(.text.module_entry)) *(.text .text.*) }
  .rodata : ALIGN(8) { *(.rodata .rodata.*) }
  .data : ALIGN(8) { *(.data .data.*) }
  .bss (NOLOAD) : ALIGN(8) { *(.bss .bss.* COMMON) }
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) }
}
