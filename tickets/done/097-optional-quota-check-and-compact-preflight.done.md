# TICKET-097: Quota-Check abschaltbar machen und Transferanzeige vereinfachen

## Nutzerwunsch

Die Speicherplatz-/Quota-Prüfung soll optional abschaltbar sein. Die Anzeige im
Kopier-/Verschiebedialog enthält zu viele Informationen und soll kompakter werden.
Im Dialog einen Verweis auf die entsprechende Einstellung anzeigen.

## Anforderungen

- Eine dauerhaft gespeicherte Einstellung für die Speicherplatz-/Quota-Prüfung
  anbieten. Bestehendes Verhalten standardmäßig beibehalten; das Abschalten
  ausdrücklich ermöglichen und verständlich beschreiben.
- Den gewählten Modus im Rust-Backend berücksichtigen, einschließlich der erneuten
  Kapazitätsprüfung vor Ausführung/Veröffentlichung. Ein ausgeschalteter Check
  darf nicht lediglich seine Anzeige verstecken.
- Andere Vorprüfungen wie Schreibrechte, ungültige Zielpfade, Namenskonflikte,
  Staging und Schutz vor Datenverlust weiterhin durchführen. Tatsächliche
  Speicherplatzfehler während der Operation weiterhin melden und sicher behandeln.
- TransferPreflight standardmäßig auf eine kurze Zusammenfassung und unmittelbar
  relevante Warnungen bzw. blockierende Probleme reduzieren. Größen lesbar mit
  Einheiten formatieren; technische Einzelwerte und zusätzliche Hinweise in einen
  aufklappbaren Detailbereich verschieben und Wiederholungen vermeiden.
- Im Dialog einen kurzen Verweis auf die Einstellungen anzeigen, möglichst als
  direkten Link zur zuständigen Einstellungsseite. Bei deaktivierter Prüfung den
  Zustand knapp kenntlich machen, ohne den Dialog mit weiteren Hinweisen zu füllen.
- Unbekannte Kapazität und bewusst deaktivierte Prüfung unterscheidbar halten.
- Deutsche und englische Texte sowie Tastaturbedienung und zugängliche
  Beschriftungen für Einstellungen, Link und Detailbereich berücksichtigen.

## Abnahme

- Tests für aktivierte/deaktivierte Prüfung, Persistenz der Einstellung und
  Behandlung tatsächlich auftretender Speicherplatzfehler.
- Backend-Tests belegen, dass die Option die Kapazitätsprüfung steuert, während
  andere Sicherheitsprüfungen erhalten bleiben.
- Dialogtests prüfen kompakte Standardansicht, aufklappbare Details und den
  Verweis auf die passende Einstellungsseite.
- `npm run check` besteht.

## Status

Umgesetzt. Die gespeicherte Einstellung steuert die Kapazitätsabfragen im Backend
bis zur Veröffentlichung; andere Sicherheitsprüfungen bleiben bestehen.
Warteschlange und Recovery übernehmen die bestätigte Einstellung. Der Dialog
zeigt eine kompakte Zusammenfassung mit Details und direktem Einstellungslink.

Tests prüfen aktivierte/deaktivierte Abfragen, erhaltene Schreib-/Staging- und
Pfadprüfungen, echten StorageFull-Fehlerpfad, Journal-Persistenz, alte Einstellungen,
Warteschlangen-Snapshots und den Dialogwechsel mit erneuter Vorprüfung.
Bedienung: [Transfer preflight](../../docs/transfer-preflight.md).
