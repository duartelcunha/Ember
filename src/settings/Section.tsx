import * as React from "react";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

/**
 * Card and row primitives shared by every settings tab.
 *
 * The tabs must fit the window without scrolling, so nothing here grows on interaction: the
 * (i) explanation opens a popover instead of expanding inside the card (which used to push the
 * cards below it off the screen), and one-line hints can be hidden by the compact density
 * variables when the window is short.
 */

/** The 16px "i" button. Radix adds aria-haspopup/aria-expanded to the trigger. */
export function InfoPopover({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`More about ${title}`}
          className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-[color:var(--border-subtle)] text-[10px] leading-none text-fg-muted transition-colors hover:text-fg data-[state=open]:border-[color:var(--border-accent)] data-[state=open]:text-fg"
        >
          i
        </button>
      </PopoverTrigger>
      <PopoverContent>{children}</PopoverContent>
    </Popover>
  );
}

export function Section({
  title,
  titleId,
  hint,
  detail,
  action,
  badge,
  elastic,
  className,
  children,
}: {
  title: string;
  /** Small pill after the title (e.g. "Primary" on the provider tried first). */
  badge?: React.ReactNode;
  /** Optional id on the title, so controls without a Label of their own can use aria-labelledby. */
  titleId?: string;
  /** ONE line. What the person needs to read to decide the toggle. */
  hint?: string;
  /** The why, the exceptions, the limits. Behind the (i) instead of on the screen: whoever is
   *  changing a setting wants to decide quickly, and whoever wants the detail knows where it is. */
  detail?: React.ReactNode;
  /** Optional control in the top-right corner of the card (e.g. "Get a key" on providers). */
  action?: React.ReactNode;
  /** This card absorbs the column's leftover height, and hands it to its body rather than to its
   *  own padding. At most one per column: two half-filled elastic regions read worse than one
   *  full one. Only for bodies that read better tall (a list, an editor, a readout, a stage). */
  elastic?: boolean;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      data-elastic={elastic ? "" : undefined}
      className={cn(
        "rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 p-[var(--card-pad,1.25rem)]",
        elastic && "flex min-h-0 flex-1 flex-col",
        className,
      )}
    >
      <div className={cn("flex items-start justify-between gap-4", elastic && "shrink-0")}>
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <h3 id={titleId} className="truncate text-sm font-semibold text-fg">{title}</h3>
            {badge}
            {detail && <InfoPopover title={title}>{detail}</InfoPopover>}
          </div>
          {hint && <p className="mt-1 text-xs text-fg-muted">{hint}</p>}
        </div>
        {action && <div className="shrink-0">{action}</div>}
      </div>
      <div className={cn("mt-[var(--card-gap,1rem)] flex flex-col gap-[var(--row-gap,1rem)]", elastic && "min-h-0 flex-1")}>
        {children}
      </div>
    </div>
  );
}

/** One row of a card: label on the LEFT, control taking the rest.
 *
 *  Labels ABOVE each field cost a line per field; with four or five fields in one card, and two
 *  cards back to back, the tab did not fit the window. Side by side, each field is one line.
 *
 *  The label column is fixed (92px) so every field lines up under the previous one; the `hint`
 *  below carries the same indent, otherwise it reads as a caption of the label instead of a
 *  caption of the field. */
export function FieldRow({
  label,
  htmlFor,
  hint,
  children,
}: {
  label: string;
  htmlFor?: string;
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-3">
        <Label htmlFor={htmlFor} className="w-[92px] shrink-0">
          {label}
        </Label>
        <div className="flex min-w-0 flex-1 items-center gap-2">{children}</div>
      </div>
      {hint && <div className="pl-[104px] text-xs text-fg-muted [display:var(--hint,block)]">{hint}</div>}
    </div>
  );
}

/** Kept under its historical name for the provider cards. */
export { FieldRow as ProviderRow };

/** A labelled switch on one line: label, optional one-line hint under it, (i) popover with
 *  the long explanation, an optional extra control (e.g. a level select) and the switch. */
export function SwitchRow({
  id,
  label,
  hint,
  detail,
  extra,
  checked,
  onCheckedChange,
  disabled,
}: {
  id: string;
  label: string;
  hint?: string;
  detail?: React.ReactNode;
  extra?: React.ReactNode;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex items-center gap-3">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <Label htmlFor={id} className="truncate">{label}</Label>
          {detail && <InfoPopover title={label}>{detail}</InfoPopover>}
        </div>
        {hint && (
          <p className="mt-0.5 truncate text-xs text-fg-muted [display:var(--hint,block)]" title={hint}>
            {hint}
          </p>
        )}
      </div>
      {extra}
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} disabled={disabled} />
    </div>
  );
}
