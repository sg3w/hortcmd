import { expect, it } from "vitest";
import { useDemo } from "@/test/tauri";
import { prepareSearch, useSearchSession } from "./searchSession";

it("keeps one session for the same caller and clears results when panel, folder or mode changes", () => {
  useDemo();
  prepareSearch("left", "/src", "files");
  useSearchSession.setState({name:"*.txt",status:"complete",ran:true});
  prepareSearch("left", "/src", "files");
  expect(useSearchSession.getState()).toMatchObject({name:"*.txt",status:"complete",ran:true});
  for (const [side,root,mode] of [["right","/src","files"],["right","/other","files"],["right","/other","duplicates"]] as const) {
    useSearchSession.setState({status:"complete",ran:true});
    prepareSearch(side,root,mode);
    expect(useSearchSession.getState()).toMatchObject({name:"*.txt",root,mode,status:null,ran:false,hits:[]});
  }
});
