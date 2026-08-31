import * as React from "react";
import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, CaretDown, CaretUp } from "@phosphor-icons/react";
import { cn } from "@/lib/utils";

export const Select = SelectPrimitive.Root;
export const SelectValue = SelectPrimitive.Value;
export const SelectGroup = SelectPrimitive.Group;

export const SelectTrigger = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(({ className, children, ...props }, ref) => (
  <SelectPrimitive.Trigger
    ref={ref}
    className={cn(
      "flex h-9 w-full items-center justify-between gap-2 rounded-sm border border-[color:var(--border-default)] bg-surface-2 px-3 text-sm text-fg outline-none transition-colors hover:bg-surface-3 focus-visible:ring-2 focus-visible:ring-[color:var(--border-accent)] disabled:cursor-not-allowed disabled:opacity-50",
      className,
    )}
    {...props}
  >
    {children}
    <SelectPrimitive.Icon asChild>
      <CaretDown size={14} className="shrink-0 text-fg-muted" />
    </SelectPrimitive.Icon>
  </SelectPrimitive.Trigger>
));
SelectTrigger.displayName = "SelectTrigger";

export const SelectContent = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content>
>(({ className, children, position = "popper", ...props }, ref) => (
  <SelectPrimitive.Portal>
    <SelectPrimitive.Content
      ref={ref}
      position={position}
      className={cn(
        // O teto de altura e o que faz a lista ter scroll. Sem ele, o Radix deixa o menu crescer
        // ate onde precisar, e como o Content e `overflow-hidden` os botoes de scroll aqui abaixo
        // nunca chegam a engatar: com uma listagem viva (16 modelos do Gemini, centenas do
        // OpenRouter) a lista passava a borda do ecra e os ultimos modelos ficavam inalcancaveis.
        // `max-h-96` e a rede para quem passe `position="item-aligned"`, onde a variavel do Radix
        // nao existe e o `min()` inteiro se anularia.
        "z-50 max-h-96 overflow-hidden rounded-md border border-[color:var(--border-default)] bg-surface-2 text-fg shadow-[var(--shadow-pop)]",
        position === "popper" &&
          "max-h-[min(24rem,var(--radix-select-content-available-height))] min-w-[var(--radix-select-trigger-width)]",
        className,
      )}
      {...props}
    >
      <SelectPrimitive.ScrollUpButton className="flex items-center justify-center py-1">
        <CaretUp size={12} />
      </SelectPrimitive.ScrollUpButton>
      {/* O scroll em si e do Radix: ele poe `overflow: hidden auto` inline no viewport, e uma
          classe nossa de overflow nunca lhe ganharia. O que faltava era so o teto de altura la em
          cima, sem o qual nunca ha nada por onde rolar. */}
      <SelectPrimitive.Viewport
        className={cn("p-1", position === "popper" && "w-full")}
      >
        {children}
      </SelectPrimitive.Viewport>
      <SelectPrimitive.ScrollDownButton className="flex items-center justify-center py-1">
        <CaretDown size={12} />
      </SelectPrimitive.ScrollDownButton>
    </SelectPrimitive.Content>
  </SelectPrimitive.Portal>
));
SelectContent.displayName = "SelectContent";

export const SelectItem = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(({ className, children, ...props }, ref) => (
  <SelectPrimitive.Item
    ref={ref}
    className={cn(
      "relative flex cursor-pointer select-none items-center rounded-sm py-1.5 pl-7 pr-3 text-sm outline-none data-[highlighted]:bg-surface-3 data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
      className,
    )}
    {...props}
  >
    <span className="absolute left-2 inline-flex h-3.5 w-3.5 items-center justify-center">
      <SelectPrimitive.ItemIndicator>
        <Check size={13} className="text-accent" weight="bold" />
      </SelectPrimitive.ItemIndicator>
    </span>
    <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
  </SelectPrimitive.Item>
));
SelectItem.displayName = "SelectItem";
