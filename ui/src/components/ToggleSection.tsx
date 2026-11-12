// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { isParamDisabled } from "../lib/collections";
import { paramHelpText } from "../lib/help";
import { HelpTip } from "./HelpTip";
import { chevron } from "./shared";

interface ToggleSectionProps {
  title: string;
  help: string;
  expanded: boolean;
  /** Each name gets a checkbox that is checked while the param is still sent. */
  params: readonly string[];
  disabledParams: Readonly<Record<string, boolean>>;
  onToggleSection: () => void;
  onToggleParam: (paramName: string) => void;
}

export function ToggleSection({
  title,
  help,
  expanded,
  params,
  disabledParams,
  onToggleSection,
  onToggleParam,
}: ToggleSectionProps) {
  return (
    <div className="param-toggles">
      <span className="label-with-help">
        <span className="expandable-toggle" onClick={onToggleSection}>
          {`${chevron(expanded)} ${title}`}
        </span>
        <HelpTip text={help} />
      </span>
      {expanded && (
        <div className="param-toggle-list">
          {/* Extra params may repeat a standard name, so names alone are not unique keys. */}
          {params.map((paramName, index) => {
            const paramHelp = paramHelpText(paramName);
            return (
              <label className="param-toggle" key={`${index}:${paramName}`}>
                <input
                  type="checkbox"
                  checked={!isParamDisabled(disabledParams, paramName)}
                  onChange={() => onToggleParam(paramName)}
                />
                <span className="param-toggle-name">{paramName}</span>
                {paramHelp !== undefined && <HelpTip text={paramHelp} />}
              </label>
            );
          })}
        </div>
      )}
    </div>
  );
}
