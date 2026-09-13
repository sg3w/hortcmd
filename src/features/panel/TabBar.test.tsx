import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { render, screen, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { useDemo } from "@/test/tauri";
import { usePanes } from "@/store/panesStore";
import { newTab } from "@/store/panes/state";
import { useSettings } from "@/store/settingsStore";
import { TabBar } from "./TabBar";

const originalScrollIntoView = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView");

beforeEach(() => {
  useDemo();
  vi.stubGlobal("ResizeObserver", class {observe() {} disconnect() {}});
  Object.defineProperty(HTMLElement.prototype,"scrollIntoView",{configurable:true,value:vi.fn()});
  usePanes.setState(s => ({left:{...s.left,tabs:[newTab("/one"),newTab("/two"),newTab("/three")]}}));
});
afterEach(() => {
  vi.unstubAllGlobals();
  if (originalScrollIntoView) Object.defineProperty(HTMLElement.prototype, "scrollIntoView", originalScrollIntoView);
  else Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});
function mount() {
  render(<><TabBar side="left" /><div role="tabpanel" id="left-files" aria-label="Files" /></>);
}

it.each(["de","en"] as const)("supports roving focus, activation and keyboard close in %s", async language => {
  useSettings.setState({language});
  const user = userEvent.setup();
  mount();
  const first = screen.getByRole("tab",{name:"one"});
  act(() => first.focus());
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("tab",{name:"two"})).toHaveFocus();
  expect(screen.getByRole("tab",{name:"two"})).toHaveAttribute("aria-selected","true");
  expect(first).toHaveAttribute("tabindex","-1");
  await user.keyboard("{End}{Delete}");
  expect(usePanes.getState().left.tabs).toHaveLength(2);
  expect(screen.getByRole("tab",{name:"two"})).toHaveFocus();
  await user.keyboard("{Home}");
  expect(first).toHaveFocus();
  const result = await axe.run(document.body,{rules:{"color-contrast":{enabled:false},region:{enabled:false}}});
  expect(result.violations.filter(v => v.impact === "serious" || v.impact === "critical").map(v => ({id:v.id,nodes:v.nodes.map(n=>n.html)}))).toEqual([]);
});

it("activates new and close buttons with Enter", async () => {
  const user = userEvent.setup();
  mount();
  act(() => screen.getByRole("button",{name:"Neuer Tab"}).focus());
  await user.keyboard("{Enter}");
  await waitFor(() => expect(usePanes.getState().left.tabs).toHaveLength(4));
  const closes = screen.getAllByRole("button",{name:/Tab schließen/});
  act(() => closes[closes.length-1].focus());
  await user.keyboard("{Enter}");
  expect(usePanes.getState().left.tabs).toHaveLength(3);
});
