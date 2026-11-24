import { ExternalLink } from "lucide-react";
import veronicaLogo from "../assets/veronica-logo.png";
import { ipc } from "../lib/ipc";
import type { Diagnostics } from "../lib/types";
import { Button, ButtonGroup, Card, KeyValueList, Page, Skeleton } from "../ui";
import "./About.css";

const SOURCE_URL = "https://github.com/namannn04/veronica";

export function AboutPage({ diagnostics }: { diagnostics: Diagnostics | null }) {
  return (
    <Page width="form" className="about-page">
      <Card className="about-card">
        <img className="about-mark" src={veronicaLogo} alt="" aria-hidden="true" />
        <div className="about-titles">
          <h1 className="about-name">Veronica</h1>
          <p className="about-tagline">A calm control center for your Linux desktop.</p>
        </div>
        <KeyValueList
          className="about-facts"
          items={[
            {
              label: "Version",
              value: diagnostics ? (
                <span className="ui-num">{diagnostics.version}</span>
              ) : (
                <Skeleton width={56} height={10} />
              ),
            },
            { label: "Platform", value: "Ubuntu and GNOME" },
            { label: "License", value: "GNU GPL v3" },
            { label: "Built with", value: "Rust, Tauri and React" },
          ]}
        />
        <ButtonGroup className="about-links">
          <Button icon={<ExternalLink />} onClick={() => void ipc.openExternal(SOURCE_URL)}>
            View source
          </Button>
          <Button
            variant="plain"
            onClick={() => void ipc.openExternal(`${SOURCE_URL}/blob/main/LICENSE`)}
          >
            Read the license
          </Button>
        </ButtonGroup>
      </Card>
    </Page>
  );
}
