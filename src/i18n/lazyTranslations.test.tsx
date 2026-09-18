import { act, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";

it("keeps core messages eager and registers feature text before the loader resolves",async()=>{
  vi.resetModules();useDemo();
  const {DICT}=await import("./dictionaries");
  expect(DICT.en["error.fs.permission"]).toBeTruthy();
  expect(DICT.en["op.cancel"]).toBeTruthy();
  for(const key of ["rename.title","preview.truncated","export.title"]) expect(DICT.en[key]).toBeUndefined();
  const {LAZY}=await import("@/features/lazyFeatures");
  expect(DICT.en["rename.title"]).toBeUndefined();
  await LAZY.rename();
  expect(DICT.en["rename.title"]).toBeTruthy();expect(DICT.de["rename.title"]).toBeTruthy();
  expect(DICT.en["export.scope"]).toBeTruthy();
  expect(DICT.en["preview.truncated"]).toBeUndefined();
  await LAZY.preview();expect(DICT.en["preview.truncated"]).toBeTruthy();
});
it("switches loaded feature text with the language without showing keys or reloading",async()=>{
  vi.resetModules();useDemo();
  const {useSettings,useT}=await import("@/store/settingsStore");
  const {LAZY}=await import("@/features/lazyFeatures");await LAZY.rename();
  const {DICT}=await import("./dictionaries");
  function Label(){const t=useT();return <span>{t("rename.title")}</span>;}
  useSettings.setState({language:"de"});render(<Label/>);
  expect(screen.getByText(DICT.de["rename.title"])).toBeInTheDocument();
  act(()=>useSettings.setState({language:"en"}));
  expect(screen.getByText(DICT.en["rename.title"])).toBeInTheDocument();
  expect(screen.queryByText("rename.title")).not.toBeInTheDocument();
});
