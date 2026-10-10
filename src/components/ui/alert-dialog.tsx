import { AlertDialog as Primitive } from '@base-ui/react/alert-dialog'
import { cn } from 'cn'
export const AlertDialog = Primitive.Root
export const AlertDialogTrigger = Primitive.Trigger
export const AlertDialogClose = Primitive.Close
export const AlertDialogTitle = Primitive.Title
export const AlertDialogDescription = Primitive.Description
export function AlertDialogContent({ className, ...props }: Primitive.Popup.Props) {
  return <Primitive.Portal>
    <Primitive.Backdrop className="fixed inset-0 z-50 bg-black/45" />
    <Primitive.Viewport className="fixed inset-0 z-50 flex items-center justify-center overflow-y-auto p-4">
      <Primitive.Popup className={cn('relative max-h-[calc(100svh-2rem)] w-full max-w-lg overflow-y-auto border bg-popover p-6 text-popover-foreground shadow-xl outline-none', className)} {...props} />
    </Primitive.Viewport>
  </Primitive.Portal>
}
