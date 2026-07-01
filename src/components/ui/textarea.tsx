import * as React from "react";
import { cn } from "@/lib/utils";

export const Textarea = React.forwardRef<
  HTMLTextAreaElement,
  React.TextareaHTMLAttributes<HTMLTextAreaElement>
>(({ className, ...props }, ref) => (
  <textarea
    ref={ref}
    className={cn(
      "w-full resize-none rounded-sm border border-[color:var(--border-default)] bg-surface-2 px-3 py-2 font-mono text-[13px] leading-relaxed text-fg outline-none placeholder:text-fg-muted focus:border-[color:var(--border-accent)]",
      className,
    )}
    {...props}
  />
));
Textarea.displayName = "Textarea";
