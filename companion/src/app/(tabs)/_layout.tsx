import Ionicons from '@expo/vector-icons/Ionicons';
import { Tabs } from 'expo-router';
import type { ColorValue } from 'react-native';

import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { isTabVisible, type TabName } from '@/state/visible-tabs';

type IconName = React.ComponentProps<typeof Ionicons>['name'];

function tabIcon(name: IconName) {
  const TabIcon = ({ color, size }: { color: ColorValue; size: number }) => (
    <Ionicons name={name} color={color} size={size} />
  );
  TabIcon.displayName = `TabIcon(${name})`;
  return TabIcon;
}

export default function TabLayout() {
  const theme = useTheme();
  const { connection, watchMode } = useWatch();

  // A tab whose screen has nothing to do is left out of the bar rather than
  // shown as something that does not work.
  const shown = (tab: TabName) =>
    isTabVisible(tab, connection, watchMode) ? {} : { href: null as never };

  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: theme.accent,
        tabBarInactiveTintColor: theme.textSecondary,
        tabBarStyle: { backgroundColor: theme.background, borderTopColor: theme.backgroundSelected },
      }}>
      <Tabs.Screen name="index" options={{ title: 'Watch', tabBarIcon: tabIcon('watch-outline') }} />
      <Tabs.Screen
        name="packages"
        options={{
          title: 'Apps',
          tabBarIcon: tabIcon('grid-outline'),
          ...shown('packages'),
        }}
      />
      <Tabs.Screen
        name="firmware"
        options={{
          title: 'Firmware',
          tabBarIcon: tabIcon('cloud-download-outline'),
          ...shown('firmware'),
        }}
      />
      <Tabs.Screen
        name="notifications"
        options={{
          title: 'Notifications',
          tabBarIcon: tabIcon('notifications-outline'),
          ...shown('notifications'),
        }}
      />
      <Tabs.Screen
        name="console"
        options={{
          title: 'Console',
          tabBarIcon: tabIcon('terminal-outline'),
          ...shown('console'),
        }}
      />
      <Tabs.Screen name="settings" options={{ title: 'Settings', tabBarIcon: tabIcon('settings-outline') }} />
    </Tabs>
  );
}
