import { useContext } from "react";
import { BootstrapResourcesContext } from "@/core/bootstrap-context";

export function useBootstrapResources() {
  return useContext(BootstrapResourcesContext);
}
