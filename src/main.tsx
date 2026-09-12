import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import { installWebviewShortcutGuards } from "./core/webview-shortcuts";
import { TooltipProvider } from "@/components/ui/tooltip";

installWebviewShortcutGuards();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider delay={150}><App /></TooltipProvider>
  </React.StrictMode>,
);
