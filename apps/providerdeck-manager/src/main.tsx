import { createRoot } from "react-dom/client";
import { App } from "./app";
import "./styles.css";

const app = document.getElementById("app");

if (app instanceof HTMLElement) {
  createRoot(app).render(<App />);
}
