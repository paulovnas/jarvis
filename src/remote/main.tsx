import { createRoot } from "react-dom/client";
import { RemoteApp } from "./RemoteApp";
import { takePairingToken } from "./client";
import "../index.css";
import "./remote.css";

const pairingToken = takePairingToken(window.location, window.history);
document.documentElement.classList.add("dark");
document.documentElement.dataset.remote = "true";
const root = document.getElementById("root");
if (root) createRoot(root).render(<RemoteApp pairingToken={pairingToken} />);
