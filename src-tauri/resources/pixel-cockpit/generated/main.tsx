
import React from "react";
import { createRoot } from "react-dom/client";
import { restoreSession } from "./jarvis-worker";
import App from "./src/App";
await restoreSession();
createRoot(document.getElementById("root")!).render(<App />);
