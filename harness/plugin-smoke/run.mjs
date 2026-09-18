// Run only in a disposable dev application identity; see plugins/README.md.
import { invoke } from "@tauri-apps/api/core";
const output = document.getElementById("result");
const source = new URLSearchParams(location.search).get("source");
const resultPath = new URLSearchParams(location.search).get("result");
if (!source || !resultPath) throw new Error("Pass source and result absolute paths");
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
const evidence=[];
async function approve(path) {
  const id=await invoke("plugin_install",{source:path,update:null});
  const record=(await invoke("list_plugins")).find(r=>r.id===id);
  if(record.active)throw new Error("Package unexpectedly active before approval");
  await invoke("plugin_activate",{plugin:id,fingerprint:record.fingerprint});return id;
}
try {
  const js=await approve(`${source}/plugins/path-tools`);
  const context={path:source,folderPath:source,filePath:null,selection:[],side:"left",copyLimit:128,copyOperation:null};
  await invoke("plugin_workspace",{workspace:{active:"left",left:context,right:{...context,side:"right"}}});
  const view=await invoke("plugin_launch",{plugin:js,button:"open"});
  let ready=false;
  for(let i=0;i<120;i++){await wait(100);const info=await invoke("plugin_info",{id:view.id});if(info.ready){ready=true;break;}}
  if(!ready)throw new Error("JS iframe did not complete the actual WebView bridge handshake");
  const received=await invoke("plugin_request",{id:view.id,request:{type:"context:get"}});
  if(received.path!==source || received.copyLimit!==128)throw new Error("Context did not match");
  evidence.push("Installed and approved JS package; command launcher opened a real WebView window; iframe sent view:ready; broker context matches.");
  const native=await approve(`${source}/plugins/native-hello/dist/org.hortcmd.native-hello`);
  await invoke("plugin_launch",{plugin:native,button:"hello"});
  let notification=false;
  for(let i=0;i<120;i++){await wait(100);const ui=await invoke("plugin_ui_poll");if(ui.events.some(e=>e.type==="notify"&&e.message==="Hello from the native Rust helper.")){notification=true;break;}}
  if(!notification)throw new Error("Native helper notification was not delivered");
  evidence.push("Installed and approved Rust package; command launcher started the real helper; notification reached the trusted UI queue.");
  for(const id of [js,native])await invoke("plugin_deactivate",{plugin:id,remove:true});
  if((await invoke("list_plugins")).length)throw new Error("Packages remain after uninstall");
  evidence.push("Both packages uninstalled and contributions removed.");
  output.textContent=JSON.stringify({ok:true,evidence},null,2);
} catch(error) {output.textContent=JSON.stringify({ok:false,error:String(error),evidence},null,2);}
await invoke("write_text_file",{path:resultPath,contents:output.textContent,overwrite:false});
