// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

export function chevron(expanded: boolean): string {
  return expanded ? "\u{25BC}" : "\u{25B6}";
}

/** Whole-row toggle for a card; the meta line only shows while the card is expanded. */
export function CardToggle({
  expanded,
  name,
  meta,
  running = false,
  onToggle,
}: {
  expanded: boolean;
  name: string;
  meta: string;
  /** Shows the running dot beside the name; the expanded card has its own status indicator. */
  running?: boolean;
  onToggle: () => void;
}) {
  return (
    <button type="button" className="card-toggle" aria-expanded={expanded} onClick={onToggle}>
      <span className="card-toggle-title">
        <span className="card-chevron">{chevron(expanded)}</span>
        <span className="server-name">{name}</span>
        {!expanded && running && <span className="status-dot running" title="running" />}
      </span>
      {expanded && <span className="server-meta card-toggle-meta">{meta}</span>}
    </button>
  );
}

export function SearchBox({
  value,
  placeholder,
  onChange,
}: {
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  return (
    <input
      className="form-input search-input"
      type="search"
      aria-label={placeholder}
      placeholder={placeholder}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

export function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div className="server-detail">
      <span className="detail-label">{label}</span>
      <span className="detail-value">{value}</span>
    </div>
  );
}

export function FormInput({
  label,
  value,
  placeholder,
  onChange,
}: {
  label: string;
  value: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="form-field">
      <span className="form-label">{label}</span>
      <input
        className="form-input"
        type="text"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}
