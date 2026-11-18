// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

/** Display order of the download list. Updater bundles and signatures match no pattern. */
const INSTALLERS = [
  { id: "macos-arm", label: "macOS (Apple Silicon)", pattern: /_aarch64\.dmg$/ },
  { id: "macos-intel", label: "macOS (Intel)", pattern: /_x64\.dmg$/ },
  { id: "windows-exe", label: "Windows (installer)", pattern: /_x64-setup\.exe$/ },
  { id: "windows-msi", label: "Windows (MSI)", pattern: /_x64_[^/]*\.msi$/ },
  { id: "linux-appimage", label: "Linux (AppImage)", pattern: /_amd64\.AppImage$/ },
  { id: "linux-deb", label: "Linux (.deb)", pattern: /_amd64\.deb$/ },
  { id: "linux-rpm", label: "Linux (.rpm)", pattern: /\.x86_64\.rpm$/ },
];

/** Maps a GitHub release's assets to installers; unrecognized assets are dropped. */
export function toDownloads(assets) {
  return INSTALLERS.flatMap(({ id, label, pattern }) => {
    const asset = assets.find(({ name }) => pattern.test(name));
    return asset === undefined ? [] : [{ id, label, url: asset.browser_download_url, size: asset.size }];
  });
}

/**
 * `architecture` comes from User-Agent Client Hints and is absent outside Chromium.
 * Mac user agents always claim Intel, so a Mac's architecture is null unless the hint says otherwise.
 */
export function detectPlatform({ userAgent, maxTouchPoints = 0, architecture = "" }) {
  const hintedArch = architecture === "arm" || architecture === "x86" ? architecture : null;
  if (/Android|iPhone|iPad|iPod/.test(userAgent)) {
    return { os: null, arch: null };
  }
  if (/Macintosh/.test(userAgent)) {
    // iPadOS Safari presents itself as a Mac, but only iPads report multiple touch points.
    return maxTouchPoints > 1 ? { os: null, arch: null } : { os: "macos", arch: hintedArch };
  }
  if (/Windows/.test(userAgent)) {
    return { os: "windows", arch: hintedArch };
  }
  if (/Linux|X11/.test(userAgent)) {
    return { os: "linux", arch: hintedArch ?? (/aarch64|arm/i.test(userAgent) ? "arm" : "x86") };
  }
  return { os: null, arch: null };
}

/**
 * The installer to offer first, plus a second choice when the Mac architecture is unknown.
 * Windows on Arm runs the x64 build under emulation; Linux on Arm has no build to offer.
 */
export function recommendDownloads(downloads, { os, arch }) {
  const byId = (id) => downloads.find((download) => download.id === id) ?? null;
  switch (os) {
    case "macos":
      if (arch === "arm") return { primary: byId("macos-arm"), alternate: null };
      if (arch === "x86") return { primary: byId("macos-intel"), alternate: null };
      return { primary: byId("macos-arm"), alternate: byId("macos-intel") };
    case "windows":
      return { primary: byId("windows-exe"), alternate: null };
    case "linux":
      return { primary: arch === "arm" ? null : byId("linux-appimage"), alternate: null };
    default:
      return { primary: null, alternate: null };
  }
}

export function formatSize(bytes) {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}
