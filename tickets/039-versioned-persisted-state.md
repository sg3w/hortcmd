# TICKET-039: Version, validate, and migrate persisted frontend state

## Goal

Ensure settings, sessions, histories, dialog sizes, and window state remain safe
and usable across application upgrades, schema changes, corruption, and invalid
values.

## Background

Several Zustand stores persist directly to localStorage, including settings,
session tabs, history, and dialog sizes. Window geometry is stored separately.
The current stores do not declare schema versions or migrations. Some setters
clamp new values, but rehydrated data can bypass setter validation, retain removed
fields, reference deleted programs, point at unavailable remote profiles, or use
invalid enum values. TICKET-029 adds a default-enabled transfer-safety setting,
which makes explicit migration behavior a data-safety requirement.

## Requirements

- Inventory every persisted key, schema, owner, retention rule, and sensitivity.
- Define an explicit monotonically increasing version for each persisted domain or
  one documented application-state envelope.
- Add deterministic migrations from every released schema version to the current
  version, including the absent-value-to-enabled rule for failure-safe transfers.
- Validate rehydrated data at runtime before it reaches application stores.
- Clamp numeric values, reject invalid enums and colors, normalize paths and file
  extensions, remove dangling program associations, and cap unbounded list sizes.
- Treat malformed or future-version data as recoverable: quarantine or export the
  original value, restore safe defaults for the affected domain, and notify the
  user once without preventing startup.
- Keep secrets out of localStorage. Connection-profile references may be persisted,
  but passwords, passphrases, tokens, and private-key contents may not.
- Handle unavailable local paths, remote profiles, displays, and window geometry
  without repeated startup errors.
- Add an in-app reset/export/import path for non-secret preferences with schema
  validation and a preview of destructive reset scope.
- Make migrations idempotent and never silently downgrade a newer schema.

## Acceptance Criteria

- [ ] Every persisted value has a documented schema version and runtime validator.
- [ ] Upgrading from all released schemas preserves valid user preferences.
- [ ] Missing `useFailureSafeTransfers` migrates to `true` as required by
      TICKET-029.
- [ ] Corrupt data cannot prevent the two-pane shell from opening.
- [ ] Invalid references and out-of-range values resolve predictably with a single
      localized notification.
- [ ] No secret is included in persisted state, exports, or migration diagnostics.
- [ ] Import rejects incompatible or tampered content without partial application.

## Tests

- Add fixture-based tests for every historical schema version and expected current
  output.
- Property-test validators with missing, extra, wrong-type, oversized, and deeply
  nested input.
- Test corrupt JSON, future versions, interrupted migrations, and repeated runs.
- Test unavailable monitors, local roots, connection profiles, editors, and tabs.
- Test export/import round trips and verify secret-like fields are rejected or
  removed.
- Test the failure-safe transfer default for fresh, legacy, explicit-on, and
  explicit-off states.

## Definition of Done

- Persisted frontend domains are versioned, validated, bounded, and migratable.
- All released schemas have regression fixtures.
- Recovery from invalid data is safe, localized, and does not block startup.
- Preference import/export excludes secrets and validates atomically.
- Schema ownership and compatibility policy are documented.
- Rename this file to `039-versioned-persisted-state.done.md`.
