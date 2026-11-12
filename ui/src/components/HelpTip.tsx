// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import { useId, useState, type MouseEvent } from "react";
import { createPortal } from "react-dom";
import { placeTooltip, type TooltipPlacement } from "../lib/help";

const TOOLTIP_WIDTH = 260;
const ESTIMATED_HEIGHT = 90;

/** An ⓘ icon that shows `text` on hover or keyboard focus. */
export function HelpTip({ text }: { text: string }) {
  const id = useId();
  const [placement, setPlacement] = useState<TooltipPlacement | null>(null);

  function show(anchor: HTMLElement) {
    setPlacement(
      placeTooltip(anchor.getBoundingClientRect(), TOOLTIP_WIDTH, ESTIMATED_HEIGHT, {
        width: window.innerWidth,
        height: window.innerHeight,
      }),
    );
  }

  // The icon sits inside labels and clickable section headers; a click must not toggle them.
  function swallow(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
  }

  return (
    <>
      <button
        type="button"
        className="help-tip"
        aria-label="Help"
        aria-describedby={placement === null ? undefined : id}
        onMouseEnter={(e) => show(e.currentTarget)}
        onMouseLeave={() => setPlacement(null)}
        onFocus={(e) => show(e.currentTarget)}
        onBlur={() => setPlacement(null)}
        onKeyDown={(e) => e.key === "Escape" && setPlacement(null)}
        onClick={swallow}
      >
        {"ⓘ"}
      </button>
      {placement !== null &&
        createPortal(
          // Portaled with fixed positioning so scrolling panels and card borders cannot clip it.
          <div id={id} role="tooltip" className="help-tooltip" style={{ ...placement, width: TOOLTIP_WIDTH }}>
            {text}
          </div>,
          document.body,
        )}
    </>
  );
}

/** A field label followed by its help icon. */
export function LabelWithHelp({ label, help, className }: { label: string; help?: string; className: string }) {
  return (
    <span className={`${className} label-with-help`}>
      {label}
      {help !== undefined && <HelpTip text={help} />}
    </span>
  );
}
