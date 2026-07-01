import React from "react";
import ReactDOM from "react-dom/client";
import { Toaster } from "sonner";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "@/styles/globals.css";
import { Settings } from "./Settings";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Settings />
    <Toaster theme="dark" position="bottom-right" richColors />
  </React.StrictMode>,
);
