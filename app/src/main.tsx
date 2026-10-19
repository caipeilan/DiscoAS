import React from "react";
import ReactDOM from "react-dom/client";
import { StartupScreen } from "./StartupScreen";
import appLogo from "../../assets/DiscoAS.svg";

const App = React.lazy(() => import("./App"));
const DiscoverWindow = React.lazy(() => import("./features/discovery/DiscoverWindow"));
const TrayMenu = React.lazy(() => import("./features/tray/TrayMenu").then((m) => ({ default: m.TrayMenu })));
const HandWindow = React.lazy(() => import("./features/hand/HandWindow").then((m) => ({ default: m.HandWindow })));
const HandDockWindow = React.lazy(() => import("./features/hand/HandDockWindow").then((m) => ({ default: m.HandDockWindow })));
const HandDragWindow = React.lazy(() => import("./features/hand/HandDragWindow").then((m) => ({ default: m.HandDragWindow })));

const favicon = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
if (favicon) favicon.href = appLogo;

const view = new URLSearchParams(location.search).get("view");
document.documentElement.classList.toggle("hand-window", view === "hand" || view === "hand-dock" || view === "hand-drag");
document.documentElement.classList.toggle("tray-menu-window", view === "tray");
document.documentElement.classList.toggle(
  "floating-window",
  view === "discover" || view === "splash" || view === "tray" || view === "hand" || view === "hand-dock" || view === "hand-drag",
);

function Root() {
  const [startupDone, setStartupDone] = React.useState(view !== "splash");
  if (view === "discover") return <DiscoverWindow />;
  if (view === "tray") return <TrayMenu />;
  if (view === "hand") return <HandWindow />;
  if (view === "hand-dock") return <HandDockWindow />;
  if (view === "hand-drag") return <HandDragWindow />;
  return startupDone ? (
    <App />
  ) : (
    <StartupScreen onComplete={() => setStartupDone(true)} />
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <React.Suspense fallback={null}><Root /></React.Suspense>
  </React.StrictMode>,
);
