import { Button, Card, CardHeader, occurrenceKeys, ProgressBar } from "@veronica/ui";
import { Blocks } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { type CardBody, type HomeCardView, homeCards, parseSurfaces } from "../lib/homeCards";
import { invoke, isDesktopApp, listenEvent } from "../lib/ipc";
import { type ExtensionRoute, extensionRoute } from "../lib/navigation";

function Body({ body }: { body: CardBody }) {
  switch (body.kind) {
    case "gauge":
      return (
        <div className="ext-card-gauge">
          <div className="ext-card-gauge-row">
            <span>{body.label}</span>
            <strong>{body.caption}</strong>
          </div>
          <ProgressBar value={body.fraction} label={body.label || body.caption} />
        </div>
      );
    case "stat":
      return (
        <div className="ext-card-stat">
          {body.label && <span className="ext-card-label">{body.label}</span>}
          <strong className="ext-card-value">{body.value}</strong>
          {body.caption && <span className="ext-card-caption">{body.caption}</span>}
        </div>
      );
    case "text":
      return <p className="ext-card-text">{body.text}</p>;
    case "list": {
      const keys = occurrenceKeys(body.items.map((item) => item.title));
      return (
        <ul className="ext-card-list">
          {body.items.map((item, index) => (
            <li key={keys[index]}>
              <span>{item.title}</span>
              {item.detail && <span className="ext-card-caption">{item.detail}</span>}
            </li>
          ))}
        </ul>
      );
    }
    case "nowPlaying":
      return (
        <div className="ext-card-stat">
          <strong className="ext-card-value now-title">{body.title}</strong>
          {body.detail && <span className="ext-card-caption now-artist">{body.detail}</span>}
          <span className="ext-card-label">{body.playing ? "Playing" : "Paused"}</span>
        </div>
      );
    default:
      return body.caption ? <p className="ext-card-text">{body.caption}</p> : null;
  }
}

export function ExtensionCards({ onOpen }: { onOpen: (route: ExtensionRoute) => void }) {
  const [cards, setCards] = useState<HomeCardView[]>([]);
  const [running, setRunning] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const load = useCallback(() => {
    if (!isDesktopApp()) {
      return;
    }
    void invoke<unknown>("ext_surfaces").then(
      (surfaces) => setCards(homeCards(parseSurfaces(surfaces))),
      () => setCards([]),
    );
  }, []);

  useEffect(() => {
    load();
    const changed = listenEvent("extension-surface", load);
    return () => {
      void changed.then((stop) => stop());
    };
  }, [load]);

  const run = async (card: HomeCardView, action: string) => {
    const key = `${card.key}/${action}`;
    setRunning(key);
    setErrors(({ [card.key]: _cleared, ...rest }) => rest);
    try {
      await invoke("ext_surface_action", {
        extension: card.extension,
        surface: card.surface,
        action,
      });
    } catch (reason) {
      setErrors((current) => ({ ...current, [card.key]: String(reason) }));
    } finally {
      setRunning(null);
    }
  };

  return (
    <>
      {cards.map((card) => (
        <Card
          key={card.key}
          as="section"
          className={["home-card", "home-span-2", "ext-card", card.blurClass]
            .filter(Boolean)
            .join(" ")}
        >
          <CardHeader
            icon={<Blocks />}
            title={card.title}
            actions={
              card.opens ? (
                <Button
                  size="small"
                  variant="plain"
                  onClick={() => onOpen(extensionRoute(card.extension, card.opens ?? ""))}
                >
                  Open
                </Button>
              ) : undefined
            }
          />
          <div className="home-card-body">
            <Body body={card.body} />
            {card.actions.length > 0 && (
              <div className="ext-card-actions">
                {card.actions.map((action) => (
                  <Button
                    key={action.id}
                    size="small"
                    busy={running === `${card.key}/${action.id}`}
                    onClick={() => void run(card, action.id)}
                  >
                    {action.label}
                  </Button>
                ))}
              </div>
            )}
            {errors[card.key] && (
              <p className="ext-card-error" role="alert">
                {errors[card.key]}
              </p>
            )}
          </div>
        </Card>
      ))}
    </>
  );
}
