import { mount } from "svelte";
import App from "./App.svelte";
import Popup from "./Popup.svelte";
import "./theme.css";

const isPopup =
  window.location.pathname === "/popup" ||
  new URLSearchParams(window.location.search).get("view") === "popup";

document.documentElement.dataset.platform = navigator.userAgent.includes("Mac")
  ? "mac"
  : navigator.userAgent.includes("Windows")
    ? "win"
    : "linux";
if (isPopup) {
  document.documentElement.classList.add("popup-view");
}

if (isPopup) {
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
