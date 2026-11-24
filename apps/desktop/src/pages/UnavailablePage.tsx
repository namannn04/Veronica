import { Construction } from "lucide-react";

import { Badge, Card, EmptyState, Page, PageHeader } from "../ui";

export function UnavailablePage({ title, description }: { title: string; description: string }) {
  return (
    <Page header={<PageHeader title={title} subtitle="Not available yet" />}>
      <Card>
        <EmptyState
          icon={<Construction />}
          title={`${title} is tracked, not simulated`}
          action={<Badge tone="warning">Not implemented yet</Badge>}
        >
          {description}
        </EmptyState>
      </Card>
    </Page>
  );
}
