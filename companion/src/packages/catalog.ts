// The packages the app can install, and how they line up with what the watch
// already has.

import { base64ToBytes } from '@/ble/encoding';
import type { AbiInfo, InstalledPackage, PackageBundle } from '@/protocol/packages';
import { incompatibilityReason } from '@/protocol/packages';

import { bundledPackages, type BundledPackage } from './bundled';

export type CatalogEntry = {
  name: string;
  label: string;
  version: string;
  kind: 'app' | 'face';
  // Present once the watch has it.
  installed: boolean;
  enabled: boolean;
  installedVersion: string | null;
  // Set when the watch could not load this package.
  blockedReason: string | null;
  bundle: BundledPackage;
};

export function decodeBundle(pkg: BundledPackage): PackageBundle {
  return {
    name: pkg.name,
    meta: pkg.meta,
    files: pkg.files.map((file) => ({
      path: file.path,
      data: base64ToBytes(file.base64),
    })),
  };
}

export function bundleSize(pkg: BundledPackage): number {
  return pkg.files.reduce((total, file) => total + base64ToBytes(file.base64).length, 0);
}

// Join what the app ships against what the watch reports.
export function buildCatalog(
  installed: InstalledPackage[],
  abi: AbiInfo | null,
  packages: BundledPackage[] = bundledPackages,
): CatalogEntry[] {
  const byName = new Map(installed.map((entry) => [entry.name, entry]));

  return packages
    .map((pkg) => {
      const match = byName.get(pkg.name);
      return {
        name: pkg.name,
        label: pkg.meta.label,
        version: pkg.meta.version,
        kind: pkg.meta.kind,
        installed: match !== undefined,
        enabled: match?.enabled ?? false,
        installedVersion: match?.version ?? null,
        blockedReason: abi ? incompatibilityReason(pkg.meta, abi) : null,
        bundle: pkg,
      };
    })
    .sort((a, b) => a.label.localeCompare(b.label));
}

// Packages the watch has that the app does not ship, so they can still be
// listed and removed.
export function strayPackages(
  installed: InstalledPackage[],
  packages: BundledPackage[] = bundledPackages,
): InstalledPackage[] {
  const known = new Set(packages.map((pkg) => pkg.name));
  return installed.filter((entry) => !known.has(entry.name));
}

export function isUpdate(entry: CatalogEntry): boolean {
  return entry.installed && entry.installedVersion !== entry.version;
}
