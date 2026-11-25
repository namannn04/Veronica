import { mountFrame } from "@veronica/ui/frame";

import { LocalPlayerProvider } from "./localPlayer";
import { MediaPage } from "./MediaPage";

mountFrame(() => (
  <LocalPlayerProvider>
    <MediaPage />
  </LocalPlayerProvider>
));
