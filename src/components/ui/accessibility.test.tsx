import { beforeEach, expect, it } from "vitest";
import { render } from "@testing-library/react";
import axe from "axe-core";
import { useDemo } from "@/test/tauri";
import { useSettings } from "@/store/settingsStore";
import { AppDialog } from "./AppDialog";
import { DriveSelect } from "@/features/panel/DriveSelect";

beforeEach(() => { useDemo(); });

async function violations() {
  // jsdom cannot measure painted colors. Contrast belongs to browser/manual QA;
  // semantic WCAG rules remain enabled for every tested component.
  const result = await axe.run(document.body, {
    runOnly:{type:"tag",values:["wcag2a","wcag2aa","wcag21aa"]},
    rules:{"color-contrast":{enabled:false}},
  });
  return result.violations.filter(v => v.impact === "serious" || v.impact === "critical")
    .map(v => ({id:v.id,nodes:v.nodes.map(n => n.html)}));
}

it.each(["de","en"] as const)("gives the location picker an accessible name in %s", async lang => {
  useSettings.setState({language:lang});
  const view = render(<DriveSelect side="left" drives={[{name:"Home",mount:"/home",total:0,free:0}]} />);
  expect(view.getByRole("combobox", {name:lang === "de" ? "Speicherort" : "Location"})).toBeInTheDocument();
  expect(await violations()).toEqual([]);
});

it.each(["de","en"] as const)("keeps the shared modal shell semantically accessible in %s", async lang => {
  useSettings.setState({language:lang});
  render(<AppDialog dialogKey="a11y" open onClose={() => {}} titleBar={lang === "de" ? "Einstellungen" : "Settings"} defaultSize={{w:400,h:300}}>
    <button>{lang === "de" ? "Speichern" : "Save"}</button>
  </AppDialog>);
  expect(await violations()).toEqual([]);
});
