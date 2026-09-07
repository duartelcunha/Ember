import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";
import { Spinner } from "./spinner";

const buttonVariants = cva(
  "relative inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-sm text-sm font-medium transition-[color,background-color,border-color,box-shadow,filter,transform] duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)] active:scale-[0.97] disabled:pointer-events-none disabled:opacity-50 disabled:active:scale-100",
  {
    variants: {
      variant: {
        // The material classes live in globals.css. Primary keeps `bg-accent` as the flat
        // colour under its gradient (the contrast test reads background-color); hover is the
        // gradient's own, so no hover colour here or it would flash through the image edges.
        primary: "ember-btn-primary bg-accent text-[color:var(--color-accent-fg)]",
        ghost:
          "ember-btn-ghost border border-[color:var(--border-subtle)] bg-surface-2 text-fg hover:bg-surface-3",
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
        {loading === undefined || asChild ? children : <>
          {/* The label stays in flow and keeps its own width, so the button never resizes when
              the spinner appears. `opacity-0` rather than `invisible`: a hidden label is dropped
              from the accessibility tree, and a busy button with no accessible name is worse
              than a briefly invisible one. The previous fix reserved a 16px slot beside the
              label instead, which held the width but pushed every idle label off centre. */}
          <span className={loading ? "opacity-0" : undefined}>{children}</span>
          {loading && <span className="absolute inset-0 grid place-items-center" aria-hidden><Spinner size={14} /></span>}
        </>}
      </Comp>
    );
  },
);
Button.displayName = "Button";
