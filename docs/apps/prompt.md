# Prompt

A question from the phone, answered Yes or No on the watch. It is a temporary app: the companion
app runs it without installing it, so it never shows in the launcher.

## Using it

- On a computer, `neotime ask "Leave for the station now?"` sends the question. The companion
  writes it to `/temp/prompt/text`, then runs the app, which opens over whatever the watch shows
  and turns the screen on.
- Tap Yes or No. The watch tells the phone the answer and goes back to the screen it covered;
  `neotime ask` prints `yes` or `no`.
- The side button goes back without answering, and so does the phone disconnecting.
- Without a question the app says the phone sent none.

## Screens

![The prompt showing the phone's question with No and Yes buttons](../../neotime/harness/snapshots/screens/app-prompt.png)

The phone's question.

![The prompt with no question](../../neotime/harness/snapshots/screens/app-prompt-empty.png)

No question from the phone.
