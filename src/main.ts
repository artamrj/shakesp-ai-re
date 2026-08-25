import { mount } from "svelte";
import App from "./App.svelte";
import Popup from "./Popup.svelte";
import "./app.css";

const path = window.location.pathname;

if (path === "/popup") {
  const app = mount(Popup, {
    target: document.getElementById("app")!,
  });
  (window as any).__app = app;
} else {
  const app = mount(App, {
    target: document.getElementById("app")!,
  });
  (window as any).__app = app;
}
