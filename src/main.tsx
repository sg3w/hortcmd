import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
const PluginWindow = React.lazy(() => import("./features/plugins/PluginFrame"));
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {new URLSearchParams(location.search).has("pluginHost")
      ? <React.Suspense fallback={null}><PluginWindow /></React.Suspense>
      : <App />}
  </React.StrictMode>,
);
