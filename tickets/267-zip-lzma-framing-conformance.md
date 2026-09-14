# TICKET-267: ZIP-LZMA-Framing mit externen Archiven prüfen

Bei TICKET-067 entdeckt: ZIP 2.4.2 verwendet einen LZMA-alone-Streamdecoder für
Methode 14. Zusätzlich fehlten Eingabeverbrauch und Abschluss des Ausgabepuffers.
Die beiden letzten Punkte sind für die Speicherbegrenzung direkt in 067 korrigiert
und mit einem gezielten Legacy-Stream/CRC-Test abgesichert.

Unabhängig davon die Methode-14-Rahmung gemäß ZIP-Spezifikation mit extern erzeugten
ZIP-LZMA-Dateien prüfen (Versionsbytes, Property-Länge, Dictionary-Properties,
End-of-stream-Markierung und deklarierte Größe). Bei Bedarf Adapter korrigieren,
ohne die Decoder-/Ausgabelimits aus 067 aufzuheben. Normale, leere, abgeschnittene
und manipulierte Dateien sowie wiederholtes Lesen und CRC prüfen.
