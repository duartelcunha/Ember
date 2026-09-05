import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";
import { Spinner } from "./spinner";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 rounded-sm text-sm font-medium transition-[color,background-color,border-color,transform] duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)] active:scale-[0.97] disabled:pointer-events-none disabled:opacity-50 disabled:active:scale-100",
  {
    variants: {
      variant: {
        primary:
          "bg-accent text-[color:var(--color-accent-fg)] hover:bg-[color:var(--color-accent-hover)]",
        ghost:
          "border border-[color:var(--border-subtle)] bg-surface-2 text-fg hover:bg-surface-3",
        outline: "border border-[color:var(--border-default)] text-fg hover:bg-surface-2",
      },
      size: { md: "h-9 px-4", sm: "h-8 px-3", icon: "h-9 w-9" },
    },
    defaultVariants: { variant: "primary", size: "md" },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
  loading?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, loading, children, disabled, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp ref={ref} type={asChild ? undefined : "button"} className={cn(buttonVariants({ variant, size }), loading && "disabled:opacity-100", className)} disabled={disabled || loading} aria-busy={loading || undefined} {...props}>
        {loading === undefined || asChild ? children : <><span className="inline-flex w-4 shrink-0 justify-center" aria-hidden>{loading && <Spinner size={14} />}</span>{children}</>}
      </Comp>
    );
  },
);
Button.displayName = "Button";
