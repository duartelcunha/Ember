import * as React from "react";
import { cn } from "@/lib/utils";

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input
      ref={ref}
      className={cn(
        "h-9 w-full rounded-sm border border-[color:var(--border-default)] bg-surface-2 px-3 text-sm text-fg outline-none placeholder:text-fg-muted focus:border-[color:var(--border-accent)]",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";
