import { Page, PageHeader } from "@veronica/ui";

import { AgentControls } from "../components/AgentControls";

export function AgentsPage() {
  return (
    <Page
      header={
        <PageHeader
          title="Agents"
          subtitle="Coding agents started from Veronica, and the ones already running"
        />
      }
    >
      <AgentControls />
    </Page>
  );
}
