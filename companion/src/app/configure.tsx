import { useLocalSearchParams, useRouter } from 'expo-router';
import { useState } from 'react';
import { Pressable, StyleSheet, Switch, TextInput, View } from 'react-native';

import { Button } from '@/components/button';
import { Card } from '@/components/card';
import { Screen } from '@/components/screen';
import { ThemedText } from '@/components/themed-text';
import { Radius, Spacing } from '@/constants/theme';
import { useTheme } from '@/hooks/use-theme';
import { useWatch } from '@/hooks/use-watch';
import { bundledPackages } from '@/packages/bundled';
import type { ConfigField } from '@/protocol/packages';
import { usePackages } from '@/state/use-packages';

// A settings form built from the package's own schema.
export default function ConfigureScreen() {
  const router = useRouter();
  const theme = useTheme();
  const { name } = useLocalSearchParams<{ name: string }>();
  const { connection, packageChannel } = useWatch();
  const packages = usePackages(packageChannel, connection === 'connected');

  const pkg = bundledPackages.find((entry) => entry.name === name);
  const fields = pkg?.meta.config ?? [];

  const [values, setValues] = useState<Record<string, unknown>>(() =>
    Object.fromEntries(fields.map((field) => [field.key, field.default])),
  );

  if (!pkg) {
    return (
      <Screen title="Settings">
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            No package named {name}.
          </ThemedText>
        </Card>
      </Screen>
    );
  }

  const set = (key: string, value: unknown) => setValues((current) => ({ ...current, [key]: value }));

  return (
    <Screen title={pkg.meta.label}>
      {fields.length === 0 ? (
        <Card>
          <ThemedText type="small" themeColor="textSecondary">
            This app has no settings.
          </ThemedText>
        </Card>
      ) : (
        <Card title="Settings">
          {fields.map((field) => (
            <Field
              key={field.key}
              field={field}
              value={values[field.key]}
              onChange={(value) => set(field.key, value)}
            />
          ))}
        </Card>
      )}

      {packages.error ? (
        <ThemedText type="small" style={{ color: theme.danger }}>
          {packages.error}
        </ThemedText>
      ) : null}

      {fields.length > 0 ? (
        <Button
          title="Save to watch"
          busy={packages.busy !== null}
          onPress={async () => {
            await packages.configure(pkg.name, values);
            router.back();
          }}
        />
      ) : null}
    </Screen>
  );
}

type FieldProps = {
  field: ConfigField;
  value: unknown;
  onChange: (value: unknown) => void;
};

function Field({ field, value, onChange }: FieldProps) {
  const theme = useTheme();

  if (field.type === 'bool') {
    return (
      <View style={styles.row}>
        <ThemedText style={styles.label}>{field.label}</ThemedText>
        <Switch
          value={Boolean(value)}
          onValueChange={onChange}
          trackColor={{ true: theme.accent, false: theme.backgroundSelected }}
          thumbColor={theme.text}
        />
      </View>
    );
  }

  if (field.type === 'int') {
    return (
      <View style={styles.row}>
        <ThemedText style={styles.label}>{field.label}</ThemedText>
        <TextInput
          value={String(value ?? '')}
          onChangeText={(text) => onChange(text === '' ? '' : Number(text.replace(/[^0-9-]/g, '')))}
          keyboardType="number-pad"
          style={[styles.input, { color: theme.text, backgroundColor: theme.backgroundSelected }]}
        />
      </View>
    );
  }

  return (
    <View style={styles.choice}>
      <ThemedText style={styles.label}>{field.label}</ThemedText>
      <View style={styles.options}>
        {(field.options ?? []).map((option) => {
          const selected = option === value;
          return (
            <Pressable key={option} onPress={() => onChange(option)}>
              <View
                style={[
                  styles.option,
                  { backgroundColor: selected ? theme.accent : theme.backgroundSelected },
                ]}>
                <ThemedText type="small" style={{ color: selected ? theme.onAccent : theme.text }}>
                  {option}
                </ThemedText>
              </View>
            </Pressable>
          );
        })}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: Spacing.three,
    paddingVertical: Spacing.two,
  },
  choice: {
    gap: Spacing.two,
    paddingVertical: Spacing.two,
  },
  label: {
    flex: 1,
  },
  input: {
    minWidth: 80,
    borderRadius: Radius.small,
    paddingHorizontal: Spacing.three,
    paddingVertical: Spacing.two,
    textAlign: 'right',
  },
  options: {
    flexDirection: 'row',
    gap: Spacing.two,
    flexWrap: 'wrap',
  },
  option: {
    paddingVertical: Spacing.two,
    paddingHorizontal: Spacing.three,
    borderRadius: Radius.small,
  },
});
