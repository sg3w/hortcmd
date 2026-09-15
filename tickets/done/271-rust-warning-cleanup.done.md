# TICKET-271: Bereinige wiederkehrende Rust- und ts-rs-Warnungen

## Auftrag
Auf Nutzerwunsch vorgezogen: Warnungen aus dem Test- und Binding-Protokoll
bereinigen, damit neue Probleme sichtbar bleiben.

## Umfang und Abnahme
- Ungenutzten 7z-Trait entfernen, Lifetime-Schreibweisen in ZIP/7z vereinheitlichen
  und den SMB-Import passend zu seinem Feature einschränken.
- ts-rs gezielt um die verwendeten Serde-Attribute ergänzen; unbekannte Attribute
  weiterhin melden. Keine globale Unterdrückung von Warnungen.
- Serde-Laufzeitvalidierung, ausgelassene Option-Felder und TypeScript-Verträge
  prüfen. Generierte Bindings dürfen sich nicht unbeabsichtigt ändern.
- Vendor-Patches dokumentieren und aktualisieren; npm run check bestehen.
- Ticket lokal committen, als .done.md ablegen und in main mergen; kein Push.

## Abschluss
- 7z/ZIP-Lifetime- und Dead-Code-Warnungen sowie featureabhängigen SMB-Import
  korrigiert; Archiv-Patch aktualisiert.
- ts-rs-macros 10.1.0 gezielt lokal ergänzt, ohne Warnungen global auszuschalten.
- Zwei IPC-Vertragstests und ein Parser-Ablehnungstest bestehen; Details und
  Herkunft in [rust-warning-cleanup.md](../../docs/rust-warning-cleanup.md).
- Abschlussprüfung: npm run check; Rust-Abschnitte ohne Warnungen und generierte
  Bindings unverändert. Frontend-Toolwarnungen bleiben gesondert (u.a. TICKET-224).
