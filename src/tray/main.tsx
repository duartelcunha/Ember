import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/geist";
import "@/styles/globals.css";
import { TrayMenu } from "./TrayMenu";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <TrayMenu />
  </React.StrictMode>,
);
