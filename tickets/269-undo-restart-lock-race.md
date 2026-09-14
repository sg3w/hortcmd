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
