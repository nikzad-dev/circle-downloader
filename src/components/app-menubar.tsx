import { Link } from '@tanstack/react-router'
import { Activity, Check, Monitor, Moon, Sun } from 'lucide-react'
import { useState } from 'react'
import { workspacePages } from '@/components/app-navigation'
import { useTheme } from '@/components/theme/theme-context'
import { Button } from '@/components/ui/button'
import {
  Popover,
  PopoverContent,
  PopoverTitle,
  PopoverTrigger,
} from '@/components/ui/popover'

const appearances = [
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
  { value: 'system', label: 'System', icon: Monitor },
] as const

export function AppMenubar() {
  const { theme, setTheme } = useTheme()
  const [open, setOpen] = useState(false)
  const ThemeIcon =
    appearances.find((appearance) => appearance.value === theme)?.icon ??
    Monitor
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <nav
        aria-label="Main navigation"
        className="flex min-w-0 items-center gap-1 overflow-x-auto py-1"
      >
        {workspacePages.map(({ to, label, icon: Icon }) => (
          <Link
            key={to}
            to={to}
            activeOptions={{ exact: to !== '/downloads', includeSearch: false }}
            search={to === '/downloads' ? {} : undefined}
            title={label}
            aria-label={label}
            className="flex h-8 shrink-0 items-center gap-2 rounded-md px-2.5 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring aria-[current=page]:bg-accent aria-[current=page]:text-accent-foreground"
          >
            <Icon className="size-4" aria-hidden="true" />
            <span className="hidden lg:inline">{label}</span>
            <span className="sr-only lg:hidden">{label}</span>
          </Link>
        ))}
      </nav>
      <div className="ml-auto shrink-0 border-l pl-2">
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger
            render={
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Appearance"
                title="Appearance"
              />
            }
          >
            <ThemeIcon className="size-4" aria-hidden="true" />
          </PopoverTrigger>
          <PopoverContent
            align="end"
            sideOffset={10}
            className="w-64 gap-3 rounded-xl"
          >
            <PopoverTitle className="text-sm normal-case">
              Appearance
            </PopoverTitle>
            <div
              role="group"
              aria-label="Color theme"
              className="grid grid-cols-3 gap-1 rounded-lg bg-muted/60 p-1"
            >
              {appearances.map(({ value, label, icon: Icon }) => (
                <Button
                  key={value}
                  variant="ghost"
                  aria-pressed={theme === value}
                  className="h-auto flex-col gap-2 rounded-md px-2 py-3 text-xs aria-pressed:bg-background aria-pressed:shadow-sm"
                  onClick={() => setTheme(value)}
                >
                  <Icon className="size-4" aria-hidden="true" />
                  {label}
                </Button>
              ))}
            </div>
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Check className="size-3" aria-hidden="true" />
              {theme === 'system'
                ? 'Matches your device appearance'
                : `${theme === 'dark' ? 'Dark' : 'Light'} appearance selected`}
            </p>
            <div className="border-t pt-2">
              <Link
                to="/health_check"
                onClick={() => setOpen(false)}
                className="flex items-center gap-2 rounded-md px-2 py-2 text-sm text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
              >
                <Activity className="size-4" aria-hidden="true" />
                System health
              </Link>
            </div>
          </PopoverContent>
        </Popover>
      </div>
    </div>
  )
}
