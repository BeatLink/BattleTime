# Snake

Snake on a 16 by 16 board, ported from wasp-os's snake. Steer the snake to the food and it grows.

## Using it

- **Swipe** up, down, left or right to steer. The snake waits until the first swipe, then moves a
  square every quarter of a second.
- Eating the food makes the snake one longer and puts new food somewhere free.
- The snake cannot turn straight back into itself.
- Running into a wall or into itself ends the game with a buzz. The screen shows the score and the
  high score, which is kept for next time.
- After a game, **tap** to play again, or **swipe** to leave.

## Screens

![The snake at the start](../../neotime/harness/snapshots/screens/app-snake.png)

The start: a one-square snake in the middle and the food below it.

![The snake after a meal](../../neotime/harness/snapshots/screens/app-snake-playing.png)

The snake two squares long after its first meal, heading right.

![Game over](../../neotime/harness/snapshots/screens/app-snake-game-over.png)

Game over after hitting the right wall, with the score and the high score.
