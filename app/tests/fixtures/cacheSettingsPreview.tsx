import { useState } from "react";
import { createRoot } from "react-dom/client";
import { CacheSettings } from "../../src/features/settings/CacheSettings";
import { emptyState, defaultGuiSettings } from "../../src/types";
import { applyAppearance } from "../../src/appearance";
import "../../src/App.css";
import "../../src/features/settings/settingsLayout.css";
applyAppearance({ ...defaultGuiSettings, followSystemTheme: false, nightMode: false });
function Preview() {
  const [value, setValue] = useState({ ...emptyState.desktopSettings, cacheDirectory: "D:\\DiscoAS\\cache" });
  return <main className="settings-content" style={{ maxWidth: 960, margin: "48px auto", padding: "0 28px" }}><CacheSettings value={value} saved={value} onChange={setValue} /></main>;
}
createRoot(document.getElementById("root")!).render(<Preview />);
