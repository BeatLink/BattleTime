import { ChoiceRow, TextRow } from '@/components/fields';
import { ThemedText } from '@/components/themed-text';
import { nativeWebBridge } from '@/services/web-bridge';
import type { CertificateTrust } from '@/services/web-request';
import type { WebAccessForm } from '@/state/web-access-form';

const TRUST_CHOICES: { value: CertificateTrust; label: string }[] = [
  { value: 'system', label: 'Usual check' },
  { value: 'pinned', label: 'This certificate' },
  { value: 'any', label: 'Any (unsafe)' },
];

// The fields for reaching a web address beyond its URL: headers, a basic-auth login and which certificate to trust.
export function WebAccessFields({ form, set }: { form: WebAccessForm; set: (patch: Partial<WebAccessForm>) => void }) {
  const native = nativeWebBridge() !== null;
  return (
    <>
      <TextRow
        label="Headers"
        detail="One Name: value per line."
        multiline
        autoCapitalize="none"
        value={form.headers}
        onChangeText={(headers) => set({ headers })}
      />
      <TextRow
        label="Username"
        detail="For a server that asks for a login (HTTP basic auth)."
        autoCapitalize="none"
        textContentType="username"
        value={form.username}
        onChangeText={(username) => set({ username })}
      />
      <TextRow
        label="Password"
        secureTextEntry
        autoCapitalize="none"
        textContentType="password"
        value={form.password}
        onChangeText={(password) => set({ password })}
      />
      <ChoiceRow<CertificateTrust>
        label="HTTPS certificate"
        detail={
          native
            ? 'For a self-signed certificate, trust that one certificate by its SHA-256 fingerprint.'
            : 'Only the usual check works here; trusting a self-signed certificate needs the desktop or Android app.'
        }
        options={TRUST_CHOICES}
        value={form.certificate}
        onChange={(certificate) => set({ certificate })}
      />
      {form.certificate === 'pinned' ? (
        <TextRow
          label="SHA-256 fingerprint"
          detail="openssl x509 -noout -fingerprint -sha256 -in cert.pem prints it."
          placeholder="AB:CD:…"
          autoCapitalize="none"
          value={form.certificateSha256}
          onChangeText={(certificateSha256) => set({ certificateSha256 })}
        />
      ) : null}
      {form.certificate === 'any' ? (
        <ThemedText type="small" themeColor="danger">
          Anyone between you and the server could read or change what is sent, including the password.
        </ThemedText>
      ) : null}
    </>
  );
}
