// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";
import { App } from "./App";
import { checkForUpdates } from "./updater";

const container = document.getElementById("app");
if (!container) {
  throw new Error("Missing #app root element");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

void checkForUpdates();
