// Notes viewer: shows, read only, the text file the companion app keeps on the watch.
// The phone writes the note to /notes/note.txt and the chosen font's name to /notes/font. The watch
// reads only the lines in view, so a long note costs no more RAM than a short one.

import { define_app } from "neotime";
import { icons } from "neotime/icons";
import { Screen, TextFile } from "neotime/widgets";

// ============================================================================
// Screen
// ============================================================================

function Note() {
  return (
    <Screen>
      <TextFile
        path="/notes/note.txt"
        font_path="/notes/font"
        placeholder="No note yet. Write or choose one in the NeoTime companion app."
        grow={1}
        padding={8}
      />
    </Screen>
  );
}

export default define_app({
  id: "notes",
  name: "Notes",
  group: "phone",
  icon: icons.note,
  screens: { main: Note },
});
