// Copyright 2026 Sam Sovereign
// SPDX-License-Identifier: Apache-2.0

export function chevron(expanded: boolean): string {
  return expanded ? "\u{25BC}" : "\u{25B6}";
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
