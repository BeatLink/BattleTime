/* The firmware's slot and the chip's RAM, from neotime-boot. */
MEMORY
{
  FLASH : ORIGIN = 0x00005000, LENGTH = 400K
  RAM : ORIGIN = 0x20000000, LENGTH = 64K
}
