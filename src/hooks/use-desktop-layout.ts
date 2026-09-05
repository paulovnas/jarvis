import { useCallback, useContext, useState } from "react";
import { DEFAULT_DESKTOP_LAYOUT, DesktopLayoutContext, type LayoutUpdate } from "@/core/desktop-layout";

export function useDesktopLayout() {
  const context = useContext(DesktopLayoutContext);
  // Isolated previews and component tests retain the same interactive behavior.
  const [local, setLocal] = useState(DEFAULT_DESKTOP_LAYOUT);
  const updateLocal = useCallback((update: LayoutUpdate) => setLocal(previous => ({ ...previous, ...(typeof update === "function" ? update(previous) : update) })), []);
  return context ?? { layout: local, updateLayout: updateLocal };
}
