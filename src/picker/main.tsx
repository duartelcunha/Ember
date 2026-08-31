import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/geist";
import "@/styles/globals.css";
import { Picker } from "./Picker";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Picker />
  </React.StrictMode>,
);
