# 2048

The sliding number game 2048, merged from InfiniTime's Twos and wasp-os's play2048. Slide the tiles
so equal numbers join, and try to make 2048.

## Using it

- **Swipe** left, right, up or down to slide every tile that way. Two equal tiles that meet join into
  one with their sum, which adds to the score.
- Each slide that moves anything adds a new tile, a 2 or, one time in ten, a 4.
- The top shows the score and your best score. Tiles take the classic 2048 colours as they grow.
- **Tap** to ask to restart, then tap **Yes** to start again or **No** to carry on.
- When no slide can move anything the game is over: the watch buzzes and the top says **Game over**.
  **Game over** replaces the best score in red. Tap to start a new game straight away.
- The board, the score and the best are kept for next time.

## Screens

![A new game](../../battletime/harness/snapshots/screens/app-twos.png)

A new game with its first two tiles, both 2s.

![A game in play](../../battletime/harness/snapshots/screens/app-twos-playing.png)

Six slides in, with a score and best of 12.

![The restart question](../../battletime/harness/snapshots/screens/app-twos-restart.png)

The **Restart game?** question a tap asks, with **No** and **Yes** buttons.

![Game over](../../battletime/harness/snapshots/screens/app-twos-game-over.png)

Game over at a score of 3928: the board is full and no slide can join two tiles.
