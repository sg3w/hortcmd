/** SVG is an image source, never injected into the application's DOM. */
export function PluginIcon({icon}: {icon: string}) {
  return icon.startsWith("data:image/")
    ? <img src={icon} alt="" aria-hidden="true" className="h-5 w-5 shrink-0 object-contain" />
    : <span aria-hidden="true" className="text-base">{icon}</span>;
}
