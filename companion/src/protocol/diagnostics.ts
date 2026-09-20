// Lines the console can run on the watch REPL with one tap.
//
// Each one imports what it needs, because the REPL namespace after an
// interrupt holds only whatever the firmware left there. Every attribute used
// here exists in the wasp-os tree: `watch.battery` is drivers/battery.py,
// `watch.rtc` is drivers/nrf_rtc.py, and `wasp.system` is the Manager in
// wasp.py.

export type QuickCommand = {
  label: string;
  detail: string;
  line: string;
  // True for a command that ends the connection, so the console can warn.
  restarts?: boolean;
};

export const QUICK_COMMANDS: QuickCommand[] = [
  {
    label: 'Diagnostics',
    detail: 'Board, battery, free memory, brightness and the time',
    line:
      'import watch, gc, wasp; print({' +
      "'board': watch.os.uname().machine, " +
      "'battery': watch.battery.level(), " +
      "'mv': watch.battery.voltage_mv(), " +
      "'charging': watch.battery.charging(), " +
      "'usb': watch.battery.power(), " +
      "'free': gc.mem_free(), " +
      "'brightness': wasp.system.brightness, " +
      "'time': watch.rtc.get_localtime()})",
  },
  {
    label: 'Battery',
    detail: 'Level, voltage and whether it is charging',
    line:
      'import watch; print(watch.battery.level(), watch.battery.voltage_mv(), watch.battery.charging())',
  },
  {
    label: 'Free memory',
    detail: 'Bytes of heap left, after collecting garbage',
    line: 'import gc; gc.collect(); print(gc.mem_free())',
  },
  {
    label: 'Clock',
    detail: 'The time the watch thinks it is',
    line: 'import watch; print(watch.rtc.get_localtime())',
  },
  {
    label: 'Uptime',
    detail: 'Seconds since the watch last restarted',
    line: 'import watch; print(watch.rtc.uptime)',
  },
  {
    label: 'Restart',
    detail: 'Reboot into the firmware',
    line: 'import machine; machine.reset()',
    restarts: true,
  },
  {
    label: 'Restart into bootloader',
    detail: 'Reboot into DFU, ready for a firmware update',
    line: 'import machine; machine.enter_ota_dfu()',
    restarts: true,
  },
];
