// Sets the watch clock from the phone's, as wasptool --rtc does; the watch keeps local time.

export function encodeSetTime(now: Date = new Date()): string {
  const fields = [
    now.getFullYear(),
    now.getMonth() + 1,
    now.getDate(),
    now.getHours(),
    now.getMinutes(),
    now.getSeconds(),
  ];
  return `import watch; watch.rtc.set_localtime((${fields.join(', ')}))\r\n`;
}
