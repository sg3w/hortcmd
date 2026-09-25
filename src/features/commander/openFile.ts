import { openPath } from "@/ipc/client";
import { describeErrorFull } from "@/store/errorText";
import { useOps } from "@/store/opsStore";
import { translate } from "@/store/settingsStore";

/** Launches through the OS and reports the same failure for F4 and Enter. */
export function openFile(path: string): void {
  void openPath(path).catch(error => {
    useOps.getState().requestConfirm({
      title: translate("open.error"),
      message: describeErrorFull(error),
      danger: true,
      onConfirm: () => {},
    });
  });
}
