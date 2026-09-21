import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import { EmojiPicker } from "./components/EmojiPicker";
import { isEmojiPickerSurface } from "./lib/surfaces";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    {isEmojiPickerSurface(window.location.search) ? <EmojiPicker /> : <App />}
  </React.StrictMode>,
);
