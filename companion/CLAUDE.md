# NeoTime Companion App

Expo (React Native) companion app for the NeoTime watch firmware in the sibling `NeoTime` repo.
Read [README.md](README.md) for the layout and the protocol.

- Screens live in `src/app` (Expo Router). Bluetooth stays behind `src/ble/transport.ts`; screens
  use the `useWatch` hook and never import `react-native-ble-plx` directly.
- Message shapes in `src/protocol/gadgetbridge.ts` mirror `NeoTime/wasp/gadgetbridge.py`. Change
  both together.
- Verify with `npm run typecheck` and `npm run lint`. BLE needs a dev build; web and Expo Go use
  the mock transport.
