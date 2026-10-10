import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { StartupScreen } from "./StartupScreen";
import { TrayMenu } from "./features/tray/TrayMenu";
import { HandWindow } from "./features/hand/HandWindow";
import appLogo from "../../assets/DiscoAS.svg";

const favicon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
if (favicon) favicon.href = appLogo;

const view = new URLSearchParams(location.search).get("view");
document.documentElement.classList.toggle("hand-window", view === "hand");
document.documentElement.classList.toggle("tray-menu-window", view === "tray");
document.documentElement.classList.toggle(
  "floating-window",
  view === "discover" || view === "splash" || view === "tray" || view === "hand",
);

function Root() {
  const [startupDone, setStartupDone] = React.useState(view !== "splash");
  if (view === "tray") return <TrayMenu />;
  if (view === "hand") return <HandWindow />;
  return startupDone ? (
    <App />
  ) : (
    <StartupScreen onComplete={() => setStartupDone(true)} />
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
