import Ionicons from '@expo/vector-icons/Ionicons';
import { Tabs } from 'expo-router';
import type { ColorValue } from 'react-native';

import { useTheme } from '@/hooks/use-theme';

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
  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: theme.accent,
        tabBarInactiveTintColor: theme.textSecondary,
        tabBarStyle: { backgroundColor: theme.background, borderTopColor: theme.backgroundSelected },
      }}>
      <Tabs.Screen name="index" options={{ title: 'Watch', tabBarIcon: tabIcon('watch-outline') }} />
      <Tabs.Screen name="packages" options={{ title: 'Apps', tabBarIcon: tabIcon('grid-outline') }} />
      <Tabs.Screen
        name="notifications"
        options={{ title: 'Notifications', tabBarIcon: tabIcon('notifications-outline') }}
      />
      <Tabs.Screen name="console" options={{ title: 'Console', tabBarIcon: tabIcon('terminal-outline') }} />
      <Tabs.Screen name="settings" options={{ title: 'Settings', tabBarIcon: tabIcon('settings-outline') }} />
    </Tabs>
  );
}
