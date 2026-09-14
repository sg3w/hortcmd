# TICKET-260: Removable-media hardware and platform verification

Follow-up to TICKET-057. The common watcher and UI are implemented; native
Windows/Linux work remains deferred under the user's scheduling instruction.

- On macOS, verify physical SD/USB insertion and safe removal, sleeping external
  drives, and mounting/unmounting an actual network share. The owned-DMG check
  does not replace these hardware scenarios.
- On Windows, verify drive letters, USB/SD media, safe removal, pending reads,
  and the home fallback. On Linux, verify udisks and manual loop mounts.
- Measure idle CPU and confirm sleeping disks stay asleep with the actual
  hardware. Polling only reads volume metadata, but OS/driver wake behaviour
  needs physical evidence.
- Verify slow or unavailable network mounts do not stall local drive updates.
- Keep directory watching, remote VFS connections and favorites unchanged.

Record the platform/hardware matrix, fix any observed defects on this ticket's
branch, and pass `npm run check` before merging.
