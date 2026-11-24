import { useEffect, useState } from "react";

import { ipc, isDesktopApp, listenEvent } from "../lib/ipc";
import type { InstalledExtension } from "../lib/types";

export function useInstalledExtensions(): InstalledExtension[] | undefined {
  const [installed, setInstalled] = useState<InstalledExtension[] | undefined>(() =>
    isDesktopApp() ? undefined : [],
  );

  useEffect(() => {
    let live = true;
    const load = () => {
      void ipc.extList().then(
        (rows) => live && setInstalled(rows),
        () => live && setInstalled((current) => current ?? []),
      );
    };
    load();
    const changed = listenEvent("extensions-changed", load);
    return () => {
      live = false;
      void changed.then((stop) => stop());
    };
  }, []);

  return installed;
}
