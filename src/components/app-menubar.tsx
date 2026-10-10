import { Link } from '@tanstack/react-router'
import { Activity, Monitor, Moon, Plus, Sun } from 'lucide-react'
import { workspacePages } from '@/components/app-navigation'
import { useTheme } from '@/components/theme/theme-context'
import { Menubar, MenubarContent, MenubarGroup, MenubarItem, MenubarLabel, MenubarMenu, MenubarRadioGroup, MenubarRadioItem, MenubarSeparator, MenubarShortcut, MenubarTrigger } from '@/components/ui/menubar'
import { strings } from '@/lib/ui-strings'

const appearances = [
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
  { value: 'system', label: 'System', icon: Monitor },
] as const

export function AppMenubar({ onAdd, dialogOpen }: { onAdd: () => void, dialogOpen: boolean }) {
  const { theme, setTheme } = useTheme()
  const modifier = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘' : 'Ctrl+'
  return (
    <Menubar aria-label="Application menu" className="h-auto gap-0 border-0 bg-transparent p-0">
      <MenubarMenu>
        <MenubarTrigger className="min-h-9 px-3 focus-visible:ring-2 focus-visible:ring-ring">File</MenubarTrigger>
        <MenubarContent finalFocus={dialogOpen ? false : undefined}>
          <MenubarItem onClick={onAdd} className="font-normal tracking-normal normal-case">
            <Plus aria-hidden="true" />
            {strings.addDownload}
            <MenubarShortcut aria-hidden="true">
              {modifier}
              N
            </MenubarShortcut>
          </MenubarItem>
        </MenubarContent>
      </MenubarMenu>
      <MenubarMenu>
        <MenubarTrigger className="min-h-9 px-3 focus-visible:ring-2 focus-visible:ring-ring">View</MenubarTrigger>
        <MenubarContent>
          <MenubarGroup>
            <MenubarLabel>Workspace</MenubarLabel>
            {workspacePages.map(({ to, label, icon: Icon }) => (
              <MenubarItem key={to} className="font-normal tracking-normal normal-case" render={<Link to={to} />}>
                <Icon aria-hidden="true" />
                {label}
              </MenubarItem>
            ))}
          </MenubarGroup>
          <MenubarSeparator />
          <MenubarGroup>
            <MenubarLabel>Appearance</MenubarLabel>
            <MenubarRadioGroup
              aria-label="Appearance"
              value={theme}
              onValueChange={(value: unknown) => {
                if (value === 'light' || value === 'dark' || value === 'system')
                  setTheme(value)
              }}
            >
              {appearances.map(({ value, label, icon: Icon }) => (
                <MenubarRadioItem key={value} value={value} closeOnClick className="text-sm font-normal tracking-normal normal-case">
                  <Icon aria-hidden="true" />
                  {label}
                </MenubarRadioItem>
              ))}
            </MenubarRadioGroup>
          </MenubarGroup>
        </MenubarContent>
      </MenubarMenu>
      <MenubarMenu>
        <MenubarTrigger className="min-h-9 px-3 focus-visible:ring-2 focus-visible:ring-ring">Help</MenubarTrigger>
        <MenubarContent>
          <MenubarItem className="font-normal tracking-normal normal-case" render={<Link to="/health_check" />}>
            <Activity aria-hidden="true" />
            System health
          </MenubarItem>
        </MenubarContent>
      </MenubarMenu>
    </Menubar>
  )
}
