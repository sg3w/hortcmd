# TICKET-128: Strukturierte Batch-Fehler im Umbenennen-Dialog anzeigen

Bei TICKET-064 aufgefallen: `RenameDialog.tsx` verwendet weiterhin
`res.errors.slice(0, 12).join("\n")`, obwohl Batch-Fehler als `ItemError[]`
transportiert werden. Dadurch kann die Fehlermeldung `[object Object]` anzeigen.

Die zentrale Funktion `describeItemErrors` verwenden; Eintragsname und übersetzte
Ursache erhalten. Partiellen Erfolg, mehrere Fehler und einen leeren Fehlerblock
prüfen. Vorhandene Umbenennungs- und Refresh-Logik nicht verändern.
