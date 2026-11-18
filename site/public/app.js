// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { detectPlatform, formatSize, recommendDownloads, toDownloads } from "./releases.js";

const LATEST_RELEASE_API = "https://api.github.com/repos/ldaIas/oauth-diag-tool/releases/latest";

async function hintedArchitecture() {
  try {
    const hints = await navigator.userAgentData?.getHighEntropyValues(["architecture"]);
    return hints?.architecture ?? "";
  } catch {
    return "";
  }
}

async function fetchLatestRelease() {
  const response = await fetch(LATEST_RELEASE_API, { headers: { Accept: "application/vnd.github+json" } });
  if (!response.ok) {
    throw new Error(`GitHub responded with ${response.status}`);
  }
  return response.json();
}

function setLink(link, download) {
  link.href = download.url;
  link.textContent = `Download for ${download.label}`;
  link.hidden = false;
}

function renderDownloadList(downloads) {
  const list = document.getElementById("download-list");
  list.replaceChildren(
    ...downloads.map((download) => {
      const link = document.createElement("a");
      link.href = download.url;
      const label = document.createElement("span");
      label.textContent = download.label;
      const size = document.createElement("span");
      size.className = "download-size";
      size.textContent = formatSize(download.size);
      link.append(label, size);
      const item = document.createElement("li");
      item.append(link);
      return item;
    }),
  );
  list.hidden = downloads.length === 0;
}

function renderRelease(release, platform) {
  const downloads = toDownloads(release.assets);
  const published = new Date(release.published_at).toLocaleDateString(undefined, { dateStyle: "long" });
  document.getElementById("release-summary").textContent = `Latest release: ${release.tag_name}, ${published}`;

  const { primary, alternate } = recommendDownloads(downloads, platform);
  // Without a matching installer the button keeps pointing at the release page.
  if (primary !== null) {
    setLink(document.getElementById("primary-download"), primary);
  }
  if (alternate !== null) {
    setLink(document.getElementById("alternate-download"), alternate);
  }
  renderDownloadList(downloads);
}

async function main() {
  const architecture = await hintedArchitecture();
  const platform = detectPlatform({
    userAgent: navigator.userAgent,
    maxTouchPoints: navigator.maxTouchPoints,
    architecture,
  });
  try {
    renderRelease(await fetchLatestRelease(), platform);
  } catch (error) {
    // The page's static link to the GitHub release page still works.
    console.error("Could not load the latest release:", error);
  }
}

void main();
