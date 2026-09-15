# TICKET-132: Lokale Archiv-Budgetpatches upstream ersetzen

TICKET-067 pflegt begrenzte Decoder-/Headerpfade in den bereits festgeschriebenen
Versionen sevenz-rust 0.6.1, ZIP 2.4.2 und tar 0.4.46. Herkunft, Patchdateien und
Grenzen stehen in docs/archive-decoder-budgets.md.

Gepflegte Releases auf vollständige Header-/Dictionary-/Metadatenbudgets prüfen;
Patches nach Möglichkeit upstream einbringen oder durch gleichwertige APIs ersetzen.
Kein bloßes Versionsupdate ohne die bösartigen Headerfixtures. Insbesondere AES
(Feature war vor 067 nicht aktiviert), direkte Schlüsselableitung, gepufferte
ZIP-LZMA-Ausgabe, CPU-Arbeitszähler und Abbruch vor erster Ausgabe berücksichtigen.
Neue Threads dürfen den threadgebundenen Budgetkontext nicht umgehen. Compiler-
Mindestversion, Lizenzen und Passwort-Kompatibilität beibehalten.
