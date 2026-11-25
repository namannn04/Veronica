import { mountFrame, useClient } from "@veronica/ui/frame";

import { ServicesPage } from "./ServicesPage";
import { SystemPage } from "./SystemPage";

function Root() {
  const client = useClient();
  return client.context.page === "services" ? <ServicesPage /> : <SystemPage />;
}

mountFrame(() => <Root />);
