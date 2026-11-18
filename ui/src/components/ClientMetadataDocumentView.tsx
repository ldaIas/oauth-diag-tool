// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useState } from "react";
import { formatClientMetadataDocument, type ClientMetadataSubject } from "../lib/clientMetadataDocument";
import { helpText } from "../lib/help";
import { LabelWithHelp } from "./HelpTip";

/** The generated metadata document with a Copy button, for hosting at the client ID URL. */
export function ClientMetadataDocumentView({ subject }: { subject: ClientMetadataSubject }) {
  const [copied, setCopied] = useState(false);
  const text = formatClientMetadataDocument(subject);

  function copy() {
    navigator.clipboard
      .writeText(text)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      })
      .catch(() => {});
  }

  return (
    <div className="auth-response-body">
      <div className="document-actions">
        <LabelWithHelp className="detail-label" label="Client Metadata Document" help={helpText("clientConfig.clientMetadataDocument")} />
        <button type="button" className="btn-import" onClick={copy}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="auth-body-content">{text}</pre>
    </div>
  );
}
