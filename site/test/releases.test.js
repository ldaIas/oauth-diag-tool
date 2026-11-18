// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import assert from "node:assert/strict";
import { test } from "node:test";
import { detectPlatform, formatSize, recommendDownloads, toDownloads } from "../public/releases.js";

const ASSET_NAMES = [
  "latest.json",
  "OAuth.Diagnostic.Tool-0.0.6-1.x86_64.rpm",
  "OAuth.Diagnostic.Tool-0.0.6-1.x86_64.rpm.sig",
  "OAuth.Diagnostic.Tool_0.0.6_aarch64.dmg",
  "OAuth.Diagnostic.Tool_0.0.6_amd64.AppImage",
  "OAuth.Diagnostic.Tool_0.0.6_amd64.AppImage.sig",
  "OAuth.Diagnostic.Tool_0.0.6_amd64.deb",
  "OAuth.Diagnostic.Tool_0.0.6_amd64.deb.sig",
  "OAuth.Diagnostic.Tool_0.0.6_x64-setup.exe",
  "OAuth.Diagnostic.Tool_0.0.6_x64-setup.exe.sig",
  "OAuth.Diagnostic.Tool_0.0.6_x64.dmg",
  "OAuth.Diagnostic.Tool_0.0.6_x64_en-US.msi",
  "OAuth.Diagnostic.Tool_0.0.6_x64_en-US.msi.sig",
  "OAuth.Diagnostic.Tool_aarch64.app.tar.gz",
  "OAuth.Diagnostic.Tool_aarch64.app.tar.gz.sig",
  "OAuth.Diagnostic.Tool_x64.app.tar.gz",
  "OAuth.Diagnostic.Tool_x64.app.tar.gz.sig",
];

const downloads = toDownloads(
  ASSET_NAMES.map((name) => ({ name, size: 1, browser_download_url: `https://example.test/${name}` })),
);

const MAC_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/18.0 Safari/605.1.15";
const WINDOWS_UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/130.0 Safari/537.36";
const LINUX_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/130.0 Safari/537.36";
const LINUX_ARM_UA = "Mozilla/5.0 (X11; Linux aarch64; rv:130.0) Gecko/20100101 Firefox/130.0";
const ANDROID_UA = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Chrome/130.0 Mobile Safari/537.36";

function recommendedIds(platformInput) {
  const { primary, alternate } = recommendDownloads(downloads, detectPlatform(platformInput));
  return [primary?.id ?? null, alternate?.id ?? null];
}

test("toDownloads keeps only installers, in display order", () => {
  assert.deepEqual(
    downloads.map(({ id }) => id),
    ["macos-arm", "macos-intel", "windows-exe", "windows-msi", "linux-appimage", "linux-deb", "linux-rpm"],
  );
  assert.equal(downloads[0].url, "https://example.test/OAuth.Diagnostic.Tool_0.0.6_aarch64.dmg");
});

test("a Mac with a known architecture gets one matching installer", () => {
  assert.deepEqual(recommendedIds({ userAgent: MAC_UA, architecture: "arm" }), ["macos-arm", null]);
  assert.deepEqual(recommendedIds({ userAgent: MAC_UA, architecture: "x86" }), ["macos-intel", null]);
});

test("a Mac with an unknown architecture is offered both installers", () => {
  assert.deepEqual(recommendedIds({ userAgent: MAC_UA }), ["macos-arm", "macos-intel"]);
});

test("Windows gets the setup executable on x86 and Arm", () => {
  assert.deepEqual(recommendedIds({ userAgent: WINDOWS_UA }), ["windows-exe", null]);
  assert.deepEqual(recommendedIds({ userAgent: WINDOWS_UA, architecture: "arm" }), ["windows-exe", null]);
});

test("x86 Linux gets the AppImage and Arm Linux gets no installer", () => {
  assert.deepEqual(recommendedIds({ userAgent: LINUX_UA }), ["linux-appimage", null]);
  assert.deepEqual(recommendedIds({ userAgent: LINUX_ARM_UA }), [null, null]);
  assert.deepEqual(recommendedIds({ userAgent: LINUX_UA, architecture: "arm" }), [null, null]);
});

test("phones and tablets get no installer", () => {
  assert.deepEqual(recommendedIds({ userAgent: ANDROID_UA }), [null, null]);
  assert.deepEqual(recommendedIds({ userAgent: MAC_UA, maxTouchPoints: 5 }), [null, null]);
});

test("a release missing the matching installer has no recommendation", () => {
  const { primary } = recommendDownloads([], { os: "windows", arch: "x86" });
  assert.equal(primary, null);
});

test("formatSize uses decimal megabytes", () => {
  assert.equal(formatSize(7_579_975), "7.6 MB");
});
