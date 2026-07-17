import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/** Combines Tailwind classes without conflicts (shadcn standard). */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
