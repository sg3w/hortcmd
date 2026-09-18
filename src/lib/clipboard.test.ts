import { afterEach, expect, it, vi } from "vitest";
import { useDemo } from "@/test/tauri";
import { writeClipboard } from "./clipboard";
function clipboard(writeText?: (text:string)=>Promise<void>) {
  useDemo();Object.defineProperty(navigator,"clipboard",{configurable:true,value:writeText ? {writeText} : undefined});
}
afterEach(()=>{delete (navigator as unknown as Record<string,unknown>).clipboard;delete (document as unknown as Record<string,unknown>).execCommand;document.body.replaceChildren();});
it("uses modern clipboard success without adding fallback nodes",async()=>{
  const writeText=vi.fn(async()=>{});clipboard(writeText);document.execCommand=vi.fn(()=>false);
  await writeClipboard("value");expect(writeText).toHaveBeenCalledWith("value");expect(document.execCommand).not.toHaveBeenCalled();expect(document.querySelector("textarea")).toBeNull();
});
it.each([false,true])("falls back after unavailable or denied modern API (%s)",async denied=>{
  clipboard(denied ? async()=>{throw new Error("denied");}:undefined);
  const field=document.createElement("input");field.value="keep selection";document.body.appendChild(field);field.focus();field.setSelectionRange(2,6,"backward");
  document.execCommand=vi.fn(()=>{expect(document.querySelector("textarea")?.value).toBe("copy value");return true;});
  await writeClipboard("copy value");expect(document.execCommand).toHaveBeenCalledWith("copy");expect(document.querySelector("textarea")).toBeNull();expect(document.activeElement).toBe(field);expect(field.selectionStart).toBe(2);expect(field.selectionEnd).toBe(6);expect(field.selectionDirection).toBe("backward");
});
it.each(["false","throw","absent"])("rejects %s without disclosing content and removes the fallback node",async mode=>{
  const secret="private clipboard canary";clipboard(async()=>{throw new Error(secret);});
  if(mode!=="absent")document.execCommand=vi.fn(()=>{if(mode==="throw")throw new Error(secret);return false;});
  const button=document.createElement("button");document.body.appendChild(button);button.focus();
  await expect(writeClipboard(secret)).rejects.toThrow("Clipboard write was refused");
  expect(document.querySelector("textarea")).toBeNull();expect(document.body.textContent).not.toContain(secret);expect(document.activeElement).toBe(button);
});
