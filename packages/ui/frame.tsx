import { type Appearance, type Client, connect, type Presenter } from "@veronica/extension-api";
import { createContext, type ReactNode, StrictMode, useContext } from "react";
import { createRoot } from "react-dom/client";

import type { VisibilitySource } from "./polling";

const ClientContext = createContext<Client | null>(null);

class FrameVisibility extends EventTarget implements VisibilitySource {
  hidden = false;

  update(visible: boolean) {
    if (this.hidden === !visible) {
      return;
    }
    this.hidden = !visible;
    this.dispatchEvent(new Event("visibilitychange"));
  }
}

export const frameVisibility = new FrameVisibility();

export function useClient(): Client {
  const client = useContext(ClientContext);
  if (client === null) {
    throw new Error("useClient is only available inside mountFrame");
  }
  return client;
}

function applyAppearance(appearance: Appearance) {
  const root = document.documentElement;
  if (appearance.theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = appearance.theme;
  }
}

function applyPresenter(presenter: Presenter) {
  document.documentElement.dataset.presenter = String(presenter.active);
}

export function mountFrame(render: () => ReactNode): void {
  const container = document.getElementById("root");
  if (container === null) {
    throw new Error("missing #root");
  }
  connect()
    .then((client) => {
      applyAppearance(client.context.appearance);
      applyPresenter(client.context.presenter);
      client.on("appearance.changed", applyAppearance);
      client.on("presenter.changed", applyPresenter);
      client.on("visibility.changed", ({ visible }) => frameVisibility.update(visible));
      createRoot(container).render(
        <StrictMode>
          <ClientContext.Provider value={client}>{render()}</ClientContext.Provider>
        </StrictMode>,
      );
    })
    .catch((reason: unknown) => console.error(reason));
}
