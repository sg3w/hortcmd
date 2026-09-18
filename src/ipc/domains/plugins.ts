import { hasTauri, invoke } from "../invoke";
import type { PluginRecord } from "../bindings/PluginRecord";
import type { PluginInstance } from "../bindings/PluginInstance";
import type { PluginRequest } from "../bindings/PluginRequest";
import type { PluginEvent } from "../bindings/PluginEvent";
export const listPlugins = () => hasTauri ? invoke<PluginRecord[]>("list_plugins") : Promise.resolve([]);
export const pluginOpen = (plugin: string, view: string | null, separate = false) => invoke<PluginInstance>("plugin_open", {plugin, view, separate});
export const pluginInfo = (id: string) => invoke<PluginInstance>("plugin_info", {id});
export const pluginRequest = (id: string, request: PluginRequest) => invoke<unknown>("plugin_request", {id, request});
export const pluginPoll = (id: string) => invoke<PluginEvent[]>("plugin_poll", {id});
export const pluginClose = (id: string) => invoke<void>("plugin_close", {id});
export const pluginAssetUrl = (id: string) => /Windows/.test(navigator.userAgent)
  ? `http://hortplugin.localhost/${id}/_entry` : `hortplugin://localhost/${id}/_entry`;
export const pluginLaunch = (plugin: string, button: string) => invoke<PluginInstance>("plugin_launch", {plugin, button});
