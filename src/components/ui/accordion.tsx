import { Accordion as AccordionPrimitive } from '@base-ui/react/accordion'
import { cn } from 'cn'
import { ChevronDown } from 'lucide-react'

function Accordion(props: AccordionPrimitive.Root.Props<string>) {
  return <AccordionPrimitive.Root data-slot="accordion" {...props} />
}

function AccordionItem({ className, ...props }: AccordionPrimitive.Item.Props) {
  return <AccordionPrimitive.Item data-slot="accordion-item" className={cn('border-b border-sidebar-border last:border-b-0', className)} {...props} />
}

function AccordionTrigger({ className, children, ...props }: AccordionPrimitive.Trigger.Props) {
  return (
    <AccordionPrimitive.Header>
      <AccordionPrimitive.Trigger data-slot="accordion-trigger" className={cn('group flex min-h-9 w-full items-center gap-2 rounded-md px-2.5 text-sm font-medium text-muted-foreground outline-none hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring', className)} {...props}>
        {children}
        <ChevronDown className="ml-auto size-4 shrink-0 transition-transform group-data-panel-open:rotate-180" aria-hidden="true" />
      </AccordionPrimitive.Trigger>
    </AccordionPrimitive.Header>
  )
}

function AccordionContent({ className, ...props }: AccordionPrimitive.Panel.Props) {
  return <AccordionPrimitive.Panel data-slot="accordion-content" className={cn('overflow-hidden pb-2', className)} {...props} />
}

export { Accordion, AccordionContent, AccordionItem, AccordionTrigger }
