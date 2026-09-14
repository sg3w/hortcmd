# TICKET-266: Native GUI-Prüfung der Dateisymbole ergänzen

Bei TICKET-065 wählte die native UI-Automation nach langer Verzögerung eine ältere
installierte hortcmd-Instanz statt des gestarteten Entwicklungsbuilds. Daher sind
native Backendmessung und Browser-Queue/Layoutprüfung getrennt dokumentiert.

Den aktuellen Build eindeutig auswählen und echte End-to-End-Messungen mit großen
lokalen Verzeichnissen ergänzen: Zeit bis bedienbare Liste, Zeit bis native Icons,
Prozessspeicher, Responsivität bei kaltem/warmem Cache und einfachen Symbolen.
Details/Liste/Miniaturen/Baum/Dateiauswahl, individuelle Ordnericons, fehlende Dateien,
Datei-/Ordner-/defekte Symlinks und beide Themes durchgehen. Mehrere Monitore bzw.
HiDPI-Skalierungswechsel und Dateizuordnungsänderung während offener Ansicht prüfen.
Reproduktionsschritte und Resultate in docs/native-file-icons.md ergänzen.
