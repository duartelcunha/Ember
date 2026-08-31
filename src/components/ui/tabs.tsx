import * as React from "react";
import * as TabsPrimitive from "@radix-ui/react-tabs";
import { motion } from "motion/react";
import { cn } from "@/lib/utils";

export const Tabs = TabsPrimitive.Root;

export const TabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.List
    ref={ref}
    className={cn(
      // `flex w-full` e nao `inline-flex`: assim a pilula tem EXATAMENTE a largura do contentor,
      // que e a mesma dos cartoes por baixo. Em inline-flex ela media-se pelo conteudo e, com
      // sete tabs, passava a largura do contentor e ficava mais comprida do que tudo o resto.
      // Assim os dois alinham por construcao, em qualquer tamanho de janela e com o numero de
      // tabs que houver, em vez de dependerem de um max-width afinado a olho.
      "flex w-full items-center gap-1 rounded-lg border border-[color:var(--border-subtle)] bg-surface-1 p-1",
      className,
    )}
    {...props}
  />
));
TabsList.displayName = "TabsList";

export const TabsTrigger = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger>
>(({ className, ...props }, ref) => (
  <TabsPrimitive.Trigger
    ref={ref}
    className={cn(
      // `flex-1` para as tabs repartirem a largura em partes iguais; `min-w-0` para o texto poder
      // encolher em vez de forcar a pilula a crescer outra vez.
      "flex min-w-0 flex-1 items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-sm font-medium text-fg-muted transition-colors duration-300 ease-out hover:text-fg focus-visible:outline-none data-[state=active]:bg-surface-3 data-[state=active]:text-fg",
      className,
    )}
    {...props}
  />
));
TabsTrigger.displayName = "TabsTrigger";

export const TabsContent = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(({ className, children, ...props }, ref) => (
  <TabsPrimitive.Content
    ref={ref}
    className={cn("mt-6 focus-visible:outline-none", className)}
    {...props}
  >
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1] }}
    >
      {children}
    </motion.div>
  </TabsPrimitive.Content>
));
TabsContent.displayName = "TabsContent";
