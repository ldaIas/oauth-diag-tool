// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

export interface ServerFormModel {
  name: string;
  /** Raw text of the port field, kept so the input is not reformatted while typing. */
  portInput: string;
  port: number;
  issuerUrl: string;
  authorizationUrl: string;
  tokenEndpoint: string;
}

const MIN_PORT = 1;
const MAX_PORT = 65535;

export function initServerForm(port: number): ServerFormModel {
  return recomputeUrls({
    name: "",
    portInput: String(port),
    port,
    issuerUrl: "",
    authorizationUrl: "",
    tokenEndpoint: "",
  });
}

export function recomputeUrls(model: ServerFormModel): ServerFormModel {
  const issuer = `http://localhost:${model.port}`;
  return {
    ...model,
    issuerUrl: issuer,
    authorizationUrl: `${issuer}/authorize`,
    tokenEndpoint: `${issuer}/token`,
  };
}

/**
 * Applies raw port-field input: non-digits are stripped, unparseable or empty
 * input keeps the previous port, and the result is clamped to the valid range.
 */
export function setServerFormPort(model: ServerFormModel, rawInput: string): ServerFormModel {
  const cleaned = rawInput.replace(/\D/g, "");
  const parsed = cleaned === "" ? Number.NaN : Number(cleaned);
  const base = Number.isSafeInteger(parsed) ? parsed : model.port;
  const port = Math.min(MAX_PORT, Math.max(MIN_PORT, base));
  return recomputeUrls({ ...model, portInput: cleaned, port });
}
