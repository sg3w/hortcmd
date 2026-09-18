/** UI review and presentation only. The backend retains request ownership and
 * performs preflight validation, journaling and the actual filesystem work. */
import { pluginTransferCancel, pluginTransferStart, pluginUiPoll } from "@/ipc/domains/plugins";
import type { PluginTransfer } from "@/ipc/bindings/PluginTransfer";
import type { RecoverySettings } from "@/ipc/bindings/RecoverySettings";
import { useOps, type ConfirmRequest } from "@/store/opsStore";
import { usePanes } from "@/store/panesStore";
import { translate, useSettings } from "@/store/settingsStore";
import { useTransfers } from "@/store/transfersStore";
import { notify } from "@/store/noticeStore";
import { ipcError } from "@/lib/appError";

export function startPluginActions() {
  let disposed=false, polling=false;
  const handled=new Set<string>();
  const approved=new Map<string,{transfer:PluginTransfer;settings:RecoverySettings;review:string}>();
  let current:{id:string;confirm:ConfirmRequest}|null=null;
  const error=(message:unknown)=>notify(usePanes.getState().active,"plugins.message",{message:String(message)},"error");
  const fail=(transfer:PluginTransfer,cause:unknown,cancelled=false)=>useTransfers.getState().finish({id:transfer.id,op:transfer.kind,ok:0,errors:cancelled?[]:[{item:"",error:ipcError(cause)}],cancelled,mode:"safe"});
  const runQueue=()=> {
    if(disposed) return;
    const state=useTransfers.getState();
    for(const id of approved.keys()) {
      if(!state.transfers.some(t=>t.id===id)) {approved.delete(id);void pluginTransferCancel(id).catch(error);}
    }
    const settings=useSettings.getState();
    if(settings.queueTransfers && state.transfers.some(t=>!t.done && !t.queued)) return;
    for(const item of state.transfers.filter(t=>t.queued && !t.done)) {
      const pending=approved.get(item.id);
      if(!pending) {if(settings.queueTransfers) break;continue;}
      approved.delete(item.id);state.setQueued(item.id,false);
      void pluginTransferStart(item.id,pending.settings,pending.review).catch(cause=>fail(pending.transfer,cause));
      if(settings.queueTransfers) break;
    }
  };
  const unsubscribe=useTransfers.subscribe(()=>queueMicrotask(runQueue));
  const review=(transfer:PluginTransfer)=> {
    handled.add(transfer.id);
    const s=useSettings.getState();
    const settings:RecoverySettings={limit:transfer.copyLimit*1024,verify:s.verifyCopies,bufKb:s.bufferSizeKb,threads:s.copyThreads,safe:s.safeTransfers,checkSpace:s.checkTransferSpace};
    const confirm:ConfirmRequest={
      title:`${transfer.pluginName}: ${translate(transfer.kind === "copy" ? "fn.copy" : "fn.move")}`,
      message:`${transfer.sources.join("\n")} → ${transfer.destination}`,
      undo:{kind:transfer.kind,sources:transfer.sources,target:transfer.destination,directory:true},
      preflight:{kind:transfer.kind,sources:transfer.sources,destination:transfer.destination,settings},
      onCancel:()=>{current=null;void pluginTransferCancel(transfer.id).catch(error);},
      onConfirm:plan=> {
        current=null;
        if(!plan) {void pluginTransferCancel(transfer.id).catch(error);return;}
        approved.set(transfer.id,{transfer,settings:plan.settings,review:plan.digest});
        useTransfers.getState().start(transfer.kind,transfer.id);
        useTransfers.getState().setQueued(transfer.id,true);
        runQueue();
      },
    };
    current={id:transfer.id,confirm};useOps.getState().requestConfirm(confirm);
  };
  const poll=async()=> {
    if(disposed || polling) return;polling=true;
    try {
      const state=await pluginUiPoll();if(disposed)return;
      for(const event of state.events) {
        if(event.type === "limit") useSettings.getState().setSpeedLimit(event.value);
        else if(event.type === "open") {const pane=usePanes.getState();void pane.loadDir(pane.active,event.path).catch(error);}
        else notify(usePanes.getState().active,"plugins.message",{message:`${event.plugin}: ${event.message}`});
      }
      const ids=new Set(state.transfers.map(t=>t.id));
      for(const id of handled) if(!ids.has(id)) handled.delete(id);
      if(current && !ids.has(current.id) && useOps.getState().confirm===current.confirm) useOps.getState().closeDialogs();
      for(const [id,item] of approved) if(!ids.has(id)) {approved.delete(id);fail(item.transfer,"Plugin transfer cancelled",true);}
      const ops=useOps.getState();
      if(!ops.confirm && !ops.prompt) {const next=state.transfers.find(t=>!handled.has(t.id));if(next)review(next);}
      runQueue();
    } catch(cause) {if(!disposed)error(cause);} finally {polling=false;}
  };
  const timer=setInterval(()=>void poll(),500);void poll();
  return()=>{disposed=true;clearInterval(timer);unsubscribe();if(current && useOps.getState().confirm===current.confirm)useOps.getState().closeDialogs();};
}
