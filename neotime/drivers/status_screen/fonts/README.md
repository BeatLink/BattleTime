`font_10x20.raw` is the X11 misc-fixed 10x20 font ("Public domain font. Share and enjoy."), as
embedded-graphics 0.8 packs it for `FONT_10X20`: the printable ASCII characters, 16 to a row, one bit
a pixel, the high bit first. `page.rs` draws from it directly, which costs the bootloader a few
hundred bytes of code where embedded-graphics' text drawing costs over 3 KB.
