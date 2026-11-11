// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { isParamDisabled } from "../lib/collections";
import { chevron } from "./shared";

interface ToggleSectionProps {
  title: string;
  expanded: boolean;
  /** Each name gets a checkbox that is checked while the param is still sent. */
  params: readonly string[];
  disabledParams: Readonly<Record<string, boolean>>;
  onToggleSection: () => void;
  onToggleParam: (paramName: string) => void;
}

export function ToggleSection({
  title,
  expanded,
  params,
  disabledParams,
  onToggleSection,
  onToggleParam,
}: ToggleSectionProps) {
  return (
    <div className="param-toggles">
      <span className="expandable-toggle" onClick={onToggleSection}>
        {`${chevron(expanded)} ${title}`}
      </span>
      {expanded && (
        <div className="param-toggle-list">
          {/* Extra params may repeat a standard name, so names alone are not unique keys. */}
          {params.map((paramName, index) => (
            <label className="param-toggle" key={`${index}:${paramName}`}>
              <input
                type="checkbox"
                checked={!isParamDisabled(disabledParams, paramName)}
                onChange={() => onToggleParam(paramName)}
              />
              <span className="param-toggle-name">{paramName}</span>
            </label>
          ))}
        </div>
      )}
    </div>
  );
}
