# TICKET-092: Fehler beim Vorbereiten paralleler Kopien nicht übergehen

## Priorität

Dringend, bei der Implementierung von 044 entdeckt und vor dessen Abschluss
vorgezogen. Zählt als Bugfix zum aktuellen Durchlauf von zehn Tickets.

## Problem

`transfer/parallel.rs::flatten_tree` ignoriert Fehler von `create_dir_all` und
`read_dir`. Ein nicht lesbarer Verzeichnisbaum kann dadurch wie ein erfolgreich
vorbereiteter leerer Baum wirken. Die Commit-Phase darf ein solches unvollständiges
Ergebnis nicht veröffentlichen oder ein bestehendes Ziel damit ersetzen.

## Anforderungen

- Fehler beim Lesen und Anlegen von Verzeichnissen an den Aufrufer weitergeben.
- Bei einem Planungsfehler keine Kopierworker oder Commit-Phase starten.
- Staging-Daten über den bestehenden Fehlerpfad behandeln; Fehler strukturiert
  melden, bisheriges Ziel und Quelle erhalten.
- Regressionstest für blockierten Zielpfad; bestehende Paralleltests erhalten.
- Keine Windows-/Linux-spezifische Implementierung oder manuelle Plattformprüfung.

## Abschluss

- [x] Fehler reproduziert und behoben.
- [x] Gezielte Rust-Tests und `npm run check` erfolgreich.
- [x] Separater lokaler Ticket-Commit und Merge nach `main`, ohne Push.

## Nachweis

Der neue Test `failed_parallel_directory_preparation_does_not_schedule_file_writes`
schlug vor der Korrektur fehl: Trotz blockiertem Zielverzeichnis wurde ein
Schreibauftrag eingeplant. Die Vorbereitung gibt jetzt `io::Result` zurück und
der Aufrufer beendet die Operation vor dem Start der Worker. Lese- und
Verzeichnisfehler sowie Aufräumfehler werden als `ItemError` weitergegeben.
