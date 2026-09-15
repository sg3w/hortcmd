# TICKET-269: Sporadische Undo-Sperre beim Wiederöffnen untersuchen

Beim vollständigen Check von TICKET-067 scheiterte der bestehende Test
`rename_survives_restart_and_refuses_changed_or_occupied_names` in
src-tauri/src/commands/fs/undo/tests.rs:42. Nach `drop(history)` liefert
`History::open` sporadisch `fs.io` mit `Resource temporarily unavailable (os error 35)`.
Die Archivtests bestanden; die Undo-Implementierung wurde in 067 nicht verändert.

Sperrlebensdauer bei parallelen Tests und Prozessstarts reproduzieren; insbesondere
kurzzeitig geerbte Deskriptoren bei fork/exec prüfen (Hypothese, nicht bestätigt).
Eine legitime zweite Instanz muss weiter ausgeschlossen werden. Keine bloße
Testwiederholung oder dauerhafte Abschaltung der Sperre als Lösung. Den
Neustart-Test parallel zu Prozessstarts stabil absichern.

## Erweiterung und Befund
Der Nutzer meldete denselben Wiederöffnungsfehler im Recovery-Test
`queue_order_settings_and_process_lock_survive_reopen`. Recovery und Undo
verwendeten beide eine nur durch File-Drop freigegebene exklusive Sperre.
Ein deterministischer Test hält einen duplizierten Deskriptor derselben
Open-File-Description offen (wie zwischen fork und exec). Ohne explizites Unlock
scheitert das Wiederöffnen reproduzierbar mit WouldBlock/os error 35.

## Abschluss
- Gemeinsamer ProcessLock für Recovery und Undo: nur erfolgreiche Akquisitionen
  erzeugen den Guard; Drop gibt die Sperre explizit frei, bevor File schließt.
- Keine Wartezeit, kein Retry und keine Abschaltung der Prozesssperre.
- Regression prüft geerbte Deskriptoren, unmittelbares Wiederöffnen, Ausschluss
  anderer Besitzer und dass ein alter Deskriptor eine neue Sperre nicht freigibt.
- Der Regressionstest scheiterte vor der Korrektur reproduzierbar und besteht
  danach. Bestehende Recovery- und Undo-Tests bleiben aktiv.
- Prüfung: npm run check sowie gezielte Wiederholungen der Wiederöffnungsfälle.
