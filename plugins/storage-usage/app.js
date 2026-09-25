/* Presentation only. All scanning, accounting, limits and authorization are Rust broker operations. */
(() => {
  const api=window.hortcmd, el=id=>document.getElementById(id);
  let disposed=false,busy=false,timer=null,batch=null,selected=0,page=0;
  const nodes=new Map();
  const text=(id,value)=>{el(id).textContent=value;};
  const bytes=value=>value==null?"Unavailable":`${value.toLocaleString()} B`;
  const value=node=>node[el("metric").value];
  const state=node=>node.incompleteBelowDepth?"Depth limited":node.incomplete?"Incomplete":batch?.status==="complete"?"Measured snapshot":"Partial scan";
  function button(label,fn){const b=document.createElement("button");b.type="button";b.textContent=label;b.onclick=fn;return b;}
  function controls(){const running=batch?.status==="running",paused=batch?.status==="paused";el("start").disabled=busy||running||paused;el("pause").disabled=busy||(!running&&!paused);el("pause").textContent=paused?"Resume":"Pause";el("cancel").disabled=busy||(!running&&!paused);}
  function zoom(id){selected=id;page=0;render();}
  function layout(items,x,y,w,h){
    if(!items.length)return;
    if(items.length===1){const n=items[0],b=button(`${n.name} · ${bytes(value(n))} · ${state(n)}`,()=>n.kind==="directory"?zoom(n.id):el("rows").scrollIntoView?.({block:"nearest"}));b.className=`tile${n.incomplete?" partial":""}`;b.style.left=`${x}%`;b.style.top=`${y}%`;b.style.width=`${w}%`;b.style.height=`${h}%`;b.title=b.textContent;el("treemap").append(b);return;}
    const total=items.reduce((sum,n)=>sum+value(n),0);let part=0,index=0;while(index<items.length-1&&part<total/2){part+=value(items[index++]);}const ratio=part/total;
    if(w>=h){layout(items.slice(0,index),x,y,w*ratio,h);layout(items.slice(index),x+w*ratio,y,w*(1-ratio),h);}else{layout(items.slice(0,index),x,y,w,h*ratio);layout(items.slice(index),x,y+h*ratio,w,h*(1-ratio));}
  }
  function render(){
    controls();if(!batch)return;
    const root=nodes.get(selected)??nodes.get(0);if(!root)return;
    text("status",`${batch.status}${root.incomplete?" · Incomplete totals":""}${batch.stale?" · Stale snapshot — rescan before relying on it":""}`);
    text("details",`${batch.scanned.toLocaleString()} entries · ${(batch.elapsedMs/1000).toFixed(1)} s · ${batch.retainedNodes}/${batch.options.nodeLimit} retained nodes · estimated scan memory ${bytes(batch.estimatedMemoryBytes)} / ${bytes(batch.memoryBudgetBytes)} budget · issues ${Object.entries(batch.issues).filter(([,v])=>v).map(([k,v])=>`${k}: ${v}`).join(", ")||"none"}`);
    text("current",batch.currentPath);text("folder",root.location);text("summary",`${bytes(value(root))} · ${root.files} files · ${root.folders} folders · ${state(root)}${root.omittedEntries?` · ${root.omittedEntries} entries aggregated (not individually retained)`:""}`);
    el("reveal").disabled=false;el("new-root").disabled=busy||batch.status==="running"||batch.status==="paused";
    const ancestors=[];let current=root;while(current){ancestors.unshift(current);current=nodes.get(current.parent);}el("breadcrumbs").replaceChildren(...ancestors.map(n=>button(n.name||"Root",()=>zoom(n.id))));
    let children=[...nodes.values()].filter(n=>n.parent===root.id);const query=el("filter").value.toLocaleLowerCase();children=children.filter(n=>n.name.toLocaleLowerCase().includes(query));
    const sort=el("sort").value;children.sort((a,b)=>sort==="name"?a.name.localeCompare(b.name):sort==="files"?b.files-a.files:(value(b)??-1)-(value(a)??-1));
    el("treemap").replaceChildren();const visible=children.filter(n=>value(n)>0),total=visible.reduce((s,n)=>s+value(n),0);const large=visible.filter((n,i)=>i<60&&value(n)>=total*.01),small=visible.filter(n=>!large.includes(n));
    if(small.length){const other={id:-1,name:`Other (${small.length} retained entries; see table)`,kind:"other",incomplete:small.some(n=>n.incomplete),[el("metric").value]:small.reduce((s,n)=>s+value(n),0)};large.push(other);}if(root.omittedBytes&&el("metric").value==="recursiveBytes")large.push({id:-2,name:"Other (unretained entries)",kind:"other",incomplete:root.incomplete,recursiveBytes:root.omittedBytes});layout(large,0,0,100,100);
    const pages=Math.max(1,Math.ceil(children.length/200));page=Math.min(page,pages-1);el("rows").replaceChildren(...children.slice(page*200,(page+1)*200).map(n=>{const row=document.createElement("tr");for(const content of [n.kind==="directory"?button(n.name,()=>zoom(n.id)):n.name,bytes(value(n)),`${n.files} / ${n.folders}`,`${n.kind} · ${state(n)}`]){const td=document.createElement("td");if(typeof content==="string")td.textContent=content;else td.append(content);row.append(td);}return row;}));
    if(root.omittedEntries){const row=document.createElement("tr"),cell=document.createElement("td");cell.colSpan=4;cell.textContent=`Other: ${root.omittedEntries} unretained entries, ${bytes(root.omittedBytes)} logical bytes. Use a narrower root for individual details.`;row.append(cell);el("rows").append(row);}
    text("page",`Page ${page+1} / ${pages}`);el("previous").disabled=page===0;el("next").disabled=page+1>=pages;
  }
  function accept(result){if(disposed)return;batch=result;for(const n of result.nodes)nodes.set(n.id,n);render();}
  function schedule(){clearTimeout(timer);if(!disposed&&batch&&(batch.status==="running"||batch.pending))timer=setTimeout(step,200);}
  async function step(){if(busy||disposed)return;busy=true;controls();try{accept(await api.request({type:"usage:step"}));}catch{batch={...batch,status:"unavailable",stale:true};text("error","Scan access changed or the provider failed. Reopen the plugin and review permissions.");render();}finally{busy=false;controls();schedule();}}
  el("scan-form").onsubmit=async event=>{event.preventDefault();if(busy||batch?.status==="running"||batch?.status==="paused"||!el("scan-form").checkValidity())return;busy=true;controls();text("error","");try{const result=await api.request({type:"usage:start",options:{sizeMode:({recursiveBytes:"logical",allocatedBytes:"allocated",deduplicatedBytes:"deduplicated"})[el("metric").value],root:el("root").value,maxDepth:Number(el("depth").value),itemLimit:Number(el("entries").value),nodeLimit:Number(el("nodes").value),seconds:Number(el("seconds").value),includeHidden:el("hidden").checked,stayOnVolume:el("volume").checked}});nodes.clear();selected=0;page=0;accept(result);}catch{text("error","Scan unavailable. Choose a supported real folder in the commander and review the limits and plugin permissions.");}finally{busy=false;controls();schedule();}};
  async function control(action){if(busy)return;busy=true;clearTimeout(timer);controls();try{accept(await api.request({type:"usage:control",action}));}catch{text("error","Could not control this scan. Its permission or instance may have changed.");}finally{busy=false;controls();schedule();}}
  el("pause").onclick=()=>control(batch?.status==="paused"?"resume":"pause");el("cancel").onclick=()=>control("cancel");
  for(const id of ["metric","sort","filter"])el(id).oninput=()=>{page=0;render();};el("previous").onclick=()=>{page--;render();};el("next").onclick=()=>{page++;render();};
  el("new-root").onclick=()=>{el("root").value=nodes.get(selected)?.location??"";el("start").focus();text("status","Review limits, then choose Start new scan. The existing snapshot remains until the new scan starts.");};
  el("reveal").onclick=async()=>{try{await api.request({type:"op:open",path:nodes.get(selected)?.location});}catch{text("error","The folder could not be revealed. It may have changed or access was revoked.");}};
  if(!api){text("error","Install and activate Storage usage in hortcmd plugin management.");el("start").disabled=true;return;}
  const unsubscribe=api.onEvent(event=>{if(event.type==="view:open"||event.type==="context:changed"){const context=event.type==="view:open"?event.data.context:event.data;if(!batch||event.type==="view:open")el("root").value=(event.type==="view:open"?event.data.path:null)||context?.folderPath||"";}if(event.type==="view:close"){disposed=true;clearTimeout(timer);unsubscribe();nodes.clear();}});
  api.request({type:"view:ready"}).then(()=>api.request({type:"context:get"})).then(context=>{if(!disposed&&!el("root").value)el("root").value=context?.folderPath??"";}).catch(()=>text("error","Plugin context is unavailable."));
})();
