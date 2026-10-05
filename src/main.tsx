import React, { lazy, Suspense } from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./index.css";
import { installWebviewShortcutGuards } from "./core/webview-shortcuts";
import { TooltipProvider } from "@/components/ui/tooltip";
import { applicationSurface } from "./core/auxiliary-windows";

installWebviewShortcutGuards();

const surface = applicationSurface("__TAURI_INTERNALS__" in window ? getCurrentWindow().label : "main");
if (surface !== "main") document.documentElement.classList.add("dark");
if (surface === "companion") document.documentElement.dataset.companion = "";
export const WindowContent = surface === "companion"
  ? lazy(() => import("./components/companion/Companion").then(module => ({ default: module.Companion })))
  : surface === "settings" || surface === "about"
    ? lazy(() => import("./components/windows/AuxiliaryWindow").then(module => ({ default: surface === "settings" ? module.SettingsWindow : module.AboutWindow })))
    : lazy(() => import("./App"));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <TooltipProvider delay={150}><Suspense fallback={null}><WindowContent /></Suspense></TooltipProvider>
  </React.StrictMode>,
);
