import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { Sprite } from "./ui/Icon";
import "./index.css";

// The states page shows every state of the window at once from fixed data. Development builds
// only: `import.meta.env.DEV` is fixed at build time, so a release build drops the page with it.
const Page = import.meta.env.DEV && location.hash === "#states"
  ? (await import("./states/StatesPage")).StatesPage
  : App;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Sprite />
    <Page />
  </StrictMode>,
);
