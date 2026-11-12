# Field help text

`en.json` holds the text shown when hovering the ⓘ icon next to a field. Entries are grouped by section, and components refer to them as `section.field` (for example `serverSettings.accessTokenExpiry`).

- To change wording, edit the value in `en.json`. No code change is needed.
- To add help to a new field, add an entry here, then pass its key as the `help` prop (`FormInput`, `Detail`, `FieldLabel`) or render `<HelpTip text={helpText("section.field")} />`.
- Entries under `param` are matched by OAuth parameter name, so a new standard parameter only needs an entry here.
- Keys are type-checked: renaming or removing an entry that a component still uses fails `npm run typecheck`.
- Keep each entry to one or two sentences.
