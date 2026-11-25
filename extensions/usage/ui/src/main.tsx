import { mountFrame, useClient } from "@veronica/ui/frame";

import { AlertsPage } from "./AlertsPage";
import { UsagePage } from "./UsagePage";

function Root() {
  const client = useClient();
  return client.context.page === "alerts" ? <AlertsPage /> : <UsagePage />;
}

mountFrame(() => <Root />);
