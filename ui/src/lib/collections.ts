// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

export function toggleMember<T>(set: ReadonlySet<T>, item: T): ReadonlySet<T> {
  const next = new Set(set);
  if (next.has(item)) {
    next.delete(item);
  } else {
    next.add(item);
  }
  return next;
}

export function withMember<T>(set: ReadonlySet<T>, item: T): ReadonlySet<T> {
  return set.has(item) ? set : new Set(set).add(item);
}

export function withoutMember<T>(set: ReadonlySet<T>, item: T): ReadonlySet<T> {
  if (!set.has(item)) {
    return set;
  }
  const next = new Set(set);
  next.delete(item);
  return next;
}

/**
 * A key that is present is removed even when its value is false, so a stored
 * `false` reads as enabled and toggling it keeps it enabled.
 */
export function toggleDisabled(params: Readonly<Record<string, boolean>>, name: string): Record<string, boolean> {
  if (Object.hasOwn(params, name)) {
    return Object.fromEntries(Object.entries(params).filter(([key]) => key !== name));
  }
  return { ...params, [name]: true };
}

export function isParamDisabled(params: Readonly<Record<string, boolean>>, name: string): boolean {
  return Object.hasOwn(params, name) && params[name] === true;
}
