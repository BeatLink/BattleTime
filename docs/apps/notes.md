# Notes

Shows a text note that the NeoTime companion app keeps on the watch, read only.

## Using it

- In the companion app's Notes screen, type the note there, choose a text file, or give a URL for
  the app to fetch every minute, and pick a font. The phone writes the note to `/notes/note.txt`
  and the font's name to `/notes/font`, again whenever it changes.
- Drag up and down to scroll through a note longer than the screen. A bar at the right edge shows
  where you are.
- Without a note the app says to choose a text file in the companion app.
- The watch reads only the lines on screen, so a long note uses no more memory than a short one.

## Screens

![The notes app showing the top of a note](../../neotime/harness/snapshots/screens/app-notes.png)

The top of a note.

![The notes app scrolled to the end of the note](../../neotime/harness/snapshots/screens/app-notes-scrolled.png)

The same note dragged up to its end.

![The notes app with no note](../../neotime/harness/snapshots/screens/app-notes-empty.png)

No note yet, until the companion app sends one.
