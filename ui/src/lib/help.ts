// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

import en from "../help/en.json";

type HelpTexts = typeof en;
type Section = keyof HelpTexts;

/** A `section.field` key from `help/en.json`; a typo or a removed entry fails the typecheck. */
export type HelpKey = { [S in Section]: `${S}.${keyof HelpTexts[S] & string}` }[Section];

export function helpText(key: HelpKey): string {
  return lookup(key) ?? "";
}

/** For OAuth parameter names, which include user-defined extra params that have no help entry. */
export function paramHelpText(paramName: string): string | undefined {
  return lookup(`param.${paramName}`);
}

function lookup(key: string): string | undefined {
  const dot = key.indexOf(".");
  const field = key.slice(dot + 1);
  const sections: Readonly<Record<string, Readonly<Record<string, string>>>> = en;
  const entries = sections[key.slice(0, dot)];
  // hasOwn keeps parameter names like "toString" from matching Object.prototype.
  return entries !== undefined && Object.hasOwn(entries, field) ? entries[field] : undefined;
}

export interface Rect {
  top: number;
  bottom: number;
  left: number;
  width: number;
}

export interface TooltipPlacement {
  left: number;
  /** Exactly one of top and bottom is set: below the anchor when it fits, otherwise above. */
  top?: number;
  bottom?: number;
}

const GAP = 6;
const MARGIN = 8;

/**
 * Fixed-position coordinates that keep a tooltip of `width` inside the viewport.
 * `estimatedHeight` only decides whether there is room below the anchor.
 */
export function placeTooltip(
  anchor: Rect,
  width: number,
  estimatedHeight: number,
  viewport: { width: number; height: number },
): TooltipPlacement {
  const centered = anchor.left + anchor.width / 2 - width / 2;
  const left = Math.max(MARGIN, Math.min(centered, viewport.width - width - MARGIN));
  const fitsBelow = anchor.bottom + GAP + estimatedHeight + MARGIN <= viewport.height;
  return fitsBelow ? { left, top: anchor.bottom + GAP } : { left, bottom: viewport.height - anchor.top + GAP };
}
