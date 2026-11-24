import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import { EmojiPicker } from "./components/EmojiPicker";
import { isEmojiPickerSurface, isFrameProbeSurface } from "./lib/surfaces";
import "./ui";

const FrameProbe = lazy(() =>
  import("./extensions/FrameProbe").then((module) => ({ default: module.FrameProbe })),
);

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root");
}

function surface() {
  if (isEmojiPickerSurface(window.location.search)) {
    return <EmojiPicker />;
  }
  if (isFrameProbeSurface(window.location.search)) {
    return (
      <Suspense fallback={null}>
        <FrameProbe />
      </Suspense>
    );
  }
  return <App />;
}

ReactDOM.createRoot(root).render(<React.StrictMode>{surface()}</React.StrictMode>);
