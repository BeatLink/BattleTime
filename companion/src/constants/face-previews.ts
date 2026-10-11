// Screenshots of the built-in watch faces from the wasp-os tree, with the white surround made transparent.
// Keyed by class name, the last part of the path the watch reports for a face.
export const FACE_PREVIEWS: Record<string, number> = {
  ChronoApp: require('../../assets/faces/ChronoApp.png'),
  ClockApp: require('../../assets/faces/ClockApp.png'),
  DualClockApp: require('../../assets/faces/DualClockApp.png'),
  FibonacciClockApp: require('../../assets/faces/FibonacciClockApp.png'),
  ResistorClockApp: require('../../assets/faces/ResistorClockApp.png'),
  WeekClockApp: require('../../assets/faces/WeekClockApp.png'),
  WordClockApp: require('../../assets/faces/WordClockApp.png'),
};

export function facePreview(path: string): number | undefined {
  return FACE_PREVIEWS[path.slice(path.lastIndexOf('.') + 1)];
}
