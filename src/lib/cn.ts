import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Kombiniert Tailwind-Klassen konfliktfrei (shadcn-Standard). */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
