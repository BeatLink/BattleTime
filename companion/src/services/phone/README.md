# Phone bridge

`phoneBridge` from `@/services/phone` tells the app about the phone's notifications, calls and
media, and lets the watch act on them. The watch side of each message is in
`src/protocol/gadgetbridge.ts`; this folder only covers the phone.

## Platforms

- **Android** uses the Expo local module in `modules/phone-bridge`. Its manifest declares the
  services and permissions, and Gradle merges them into the app, so no config plugin is needed.
  The module is native code, so it needs a development build.
- **iOS, web and Expo Go** get a stub with `available: false`. Queries answer `false`, actions
  reject, and `subscribe` never calls back. iOS has no public way to read other apps'
  notifications; that would need ANCS on the watch.

## What the Android module does

- **Notifications.** A `NotificationListenerService` sends `notification` and
  `notificationRemoved` events once the user turns on notification access
  (`openNotificationAccessSettings`). It skips ongoing, group summary, media and call
  notifications, and the app's own. The id is a hash of the notification's key, so a removal
  carries the same id as its post. An app that updates a notification posts it again with the same
  id.
- **Media.** The same service follows the active media session, preferring one that is playing,
  and sends `media` when the track, play state or position (on a seek) changes. A late subscriber
  gets the last state straight away. `mediaCommand` drives that session, or sends a media key when
  there is none; volume goes to the music stream, or to the session on a cast device.
- **Calls.** After `requestCallPermissions`, call state arrives as `call` events: `incoming`,
  `outgoing`, `start` (an incoming call answered) and `end`. The number needs the call log
  permission and the name needs contacts. `answerCall` needs Android 8 and `rejectCall` Android 9.
- **Do Not Disturb.** `isDoNotDisturb` is true when the interruption filter is anything but "all".
  Nothing is filtered natively; the app decides what to forward.
- **Keep-alive.** `startKeepAlive` runs a foreground service of type `connectedDevice` with an
  ongoing notification, so the Bluetooth link in JavaScript survives the app being backgrounded.
  Call it while the app is in front, because Android refuses to start one from the background.

The Kotlin has never been compiled or run.
