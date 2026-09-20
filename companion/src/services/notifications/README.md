# Notification forwarding

Forwarding phone notifications to the watch needs platform code that Expo does not ship:

- **Android** needs a `NotificationListenerService` exposed as an Expo module. It should emit
  posted and removed notifications to JavaScript, which maps them onto the `notify` and `notify-`
  messages in `src/protocol/gadgetbridge.ts`.
- **iOS** offers no public API to read other apps' notifications. The Apple Notification Center
  Service (ANCS) would have to be implemented on the watch side instead.

Media control from the watch (`music` messages) similarly needs a media session bridge on each
platform. Both live here once written, behind the toggles on the Notifications tab.
