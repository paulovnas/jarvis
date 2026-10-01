import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./index.css";
import { installWebviewShortcutGuards } from "./core/webview-shortcuts";
import { TooltipProvider } from "@/components/ui/tooltip";

installWebviewShortcutGuards();

const companion = "__TAURI_INTERNALS__" in window && getCurrentWindow().label === "companion";
if (companion) { document.documentElement.dataset.companion = ""; document.documentElement.classList.add("dark"); }
export const WindowContent = companion
  ? lazy(() => import("./components/companion/Companion").then(module => ({ default: module.Companion })))
  : lazy(() => import("./App"));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider delay={150}><Suspense fallback={null}><WindowContent /></Suspense></TooltipProvider>
  </React.StrictMode>,
);
