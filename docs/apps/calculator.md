# Calculator

A calculator for adding, subtracting, multiplying and dividing numbers of up to fifteen digits, six
of them after the point. Merged from wasp-os's calculator and InfiniTime's Calculator.

## Using it

- Tap the **digit** keys and the **.** key to type a number. The number being typed shows in large
  text; the result so far and the waiting operation show small above it.
- Tap **+**, **-**, **×** or **÷** to apply the waiting operation and choose the next one, whose key
  stays lit.
- Tap **=** to work out the sum. Tapping **=** again repeats the last operation.
- Tap **±** to change the sign of the number being typed.
- Tap **←** to delete the last digit typed; with nothing typed it clears the result.
- Tap **C** to clear everything.
- A result that is too large shows "too large" in red, and a division by zero "zero division"; the
  watch buzzes, and the next key starts afresh.
- The sum is kept when the app is closed, ready to carry on next time.

## Screens

![The calculator, cleared](../../battletime/harness/snapshots/screens/app-calculator.png)

Cleared, showing zero.

![The calculator partway through a sum](../../battletime/harness/snapshots/screens/app-calculator-sum.png)

Partway through 12 + 5: the result and the operation above, the number being typed below.

![The calculator after 3.5 × 8 =](../../battletime/harness/snapshots/screens/app-calculator-result.png)

After 3.5 × 8 =: the result 28 above, with the last number and operation kept so that = repeats.

![The calculator after dividing by zero](../../battletime/harness/snapshots/screens/app-calculator-zero-division.png)

After 8 ÷ 0 =, the red "zero division" message.
