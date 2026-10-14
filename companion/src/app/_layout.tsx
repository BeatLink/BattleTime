import { DarkTheme, DefaultTheme, Stack, ThemeProvider } from 'expo-router';
import * as SplashScreen from 'expo-splash-screen';
import { StatusBar } from 'expo-status-bar';
import { useEffect } from 'react';

import { Colors } from '@/constants/theme';
import { useColorScheme } from '@/hooks/use-color-scheme';
import { WatchProvider } from '@/state/watch-provider';

SplashScreen.preventAutoHideAsync();

export default function RootLayout() {
  const scheme = useColorScheme();
  const dark = scheme !== 'light';
  const palette = dark ? Colors.dark : Colors.light;
  const base = dark ? DarkTheme : DefaultTheme;
  const theme = {
    ...base,
    colors: {
      ...base.colors,
      primary: palette.accent,
      background: palette.background,
      card: palette.backgroundElement,
      text: palette.text,
      border: palette.backgroundSelected,
    },
  };

  useEffect(() => {
    SplashScreen.hideAsync();
  }, []);

  return (
    <ThemeProvider value={theme}>
      <WatchProvider>
        <StatusBar style={dark ? 'light' : 'dark'} />
        <Stack screenOptions={{ headerShown: false }}>
          <Stack.Screen name="(tabs)" />
          <Stack.Screen name="scan" options={{ presentation: 'modal', headerShown: true, title: 'Find a watch' }} />
          <Stack.Screen name="steps" options={{ headerShown: true, title: 'Steps' }} />
          <Stack.Screen name="alarms" options={{ headerShown: true, title: 'Alarms' }} />
          <Stack.Screen name="watch-settings" options={{ headerShown: true, title: 'Watch settings' }} />
          <Stack.Screen name="files" options={{ headerShown: true, title: 'Files' }} />
          <Stack.Screen name="notes" options={{ headerShown: true, title: 'Notes' }} />
          <Stack.Screen name="remote" options={{ headerShown: true, title: 'Remote' }} />
          <Stack.Screen name="quick-apps" options={{ headerShown: true, title: 'Quick apps' }} />
          <Stack.Screen name="backup" options={{ headerShown: true, title: 'Backup' }} />
          <Stack.Screen name="apps" options={{ headerShown: true, title: 'Apps' }} />
          <Stack.Screen name="launcher" options={{ headerShown: true, title: 'Launcher' }} />
          <Stack.Screen name="setup" options={{ headerShown: true, title: 'Set up the watch' }} />
        </Stack>
      </WatchProvider>
    </ThemeProvider>
  );
}
