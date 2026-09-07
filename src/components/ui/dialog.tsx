import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "@phosphor-icons/react";
import { cn } from "@/lib/utils";

/**
 * Modal dialog for everything that used to be an inline disclosure (previous profile, import
 * review, context details, capture timing, logs). The settings window never scrolls as a page,
 * so long or optional content opens on top of the tab instead of pushing it down.
 *
 * Only `DialogBody` scrolls. The content box is capped at the viewport minus 24px on each side
 * so a dialog never grows past a small window; the header and footer stay put.
 */
export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;

type DialogSize = "md" | "lg";
const DIALOG_WIDTH: Record<DialogSize, string> = { md: "640px", lg: "900px" };

export const DialogContent = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Content> & { size?: DialogSize }
>(({ className, children, size = "md", style, ...props }, ref) => (
  <DialogPrimitive.Portal>
    <DialogPrimitive.Overlay className="ember-dialog-overlay fixed inset-0 z-50 bg-[color:var(--overlay-scrim)]" />
    <DialogPrimitive.Content
      ref={ref}
      style={{ "--dialog-w": DIALOG_WIDTH[size], ...style } as React.CSSProperties}
      className={cn(
        "ember-dialog fixed left-1/2 top-1/2 z-50 flex max-h-[calc(100vh-48px)] w-[min(var(--dialog-w),calc(100vw-48px))] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-lg border border-[color:var(--border-default)] bg-surface-1 p-5 text-fg shadow-[var(--shadow-pop)] outline-none",
        className,
      )}
      {...props}
    >
      {children}
      <DialogPrimitive.Close
        aria-label="Close"
        className="absolute right-3 top-3 grid h-7 w-7 place-items-center rounded-md text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)]"
      >
        <X size={14} weight="bold" />
      </DialogPrimitive.Close>
    </DialogPrimitive.Content>
  </DialogPrimitive.Portal>
));
DialogContent.displayName = "DialogContent";

export function DialogHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-col gap-1 pr-8", className)} {...props} />;
}

export const DialogTitle = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Title>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Title>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Title ref={ref} className={cn("text-sm font-semibold text-fg", className)} {...props} />
));
DialogTitle.displayName = "DialogTitle";

export const DialogDescription = React.forwardRef<
  React.ElementRef<typeof DialogPrimitive.Description>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Description>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Description ref={ref} className={cn("text-xs text-fg-muted", className)} {...props} />
));
DialogDescription.displayName = "DialogDescription";

/** The one scrolling region of a dialog. `data-scroll-pane` marks it for the layout tests. */
export function DialogBody({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      data-scroll-pane=""
      className={cn("min-h-0 flex-1 overflow-y-auto text-sm", className)}
      {...props}
    />
  );
}

export function DialogFooter({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("flex flex-wrap justify-end gap-2", className)} {...props} />;
}
