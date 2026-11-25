import { Page, PageHeader } from "@veronica/ui";
import { mountFrame, useClient } from "@veronica/ui/frame";

import { CleanerPane } from "./CleanerPane";
import { MaintenancePage } from "./MaintenancePage";

function CleanerPage() {
  return (
    <Page
      header={
        <PageHeader
          title="Disk cleaner"
          subtitle="Developer caches and build output, moved to the Trash only when you say so"
        />
      }
    >
      <CleanerPane />
    </Page>
  );
}

function Root() {
  const client = useClient();
  return client.context.page === "cleaner" ? <CleanerPage /> : <MaintenancePage />;
}

mountFrame(() => <Root />);
