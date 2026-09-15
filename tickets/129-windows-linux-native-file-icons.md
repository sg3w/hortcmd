# TICKET-129: Native Dateisymbole unter Windows und Linux

Aus TICKET-065 auf Benutzerwunsch verschoben: keine Windows-/Linux-Arbeit in der
aktuellen Feature-Reihe. Gemeinsame IPC-Typen, Queue, LRU, Fallbacks und Einstellung
sind vorhanden. Derzeit liefern diese Plattformen einfache Symbole.

Windows-Shell- und Linux-Desktop-Adapter mit gepflegten Bindings ergänzen; COM- bzw.
GUI-Threadregeln, HiDPI/Theme, individuelle Icons und native Link-Overlays beachten.
Remote-/Archivorte ausschließlich lokal nach Typ auflösen. Limits des vorhandenen
Transports beibehalten. Native Tests einschließlich Linux ohne Desktop/Icontheme,
Windows-Verknüpfungen, defekter Links und warmer/kalter großer Verzeichnisse ausführen.
