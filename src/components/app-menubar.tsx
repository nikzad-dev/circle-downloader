import { Link } from '@tanstack/react-router'
import { Activity, Monitor, Moon, Sun } from 'lucide-react'
import { workspacePages } from '@/components/app-navigation'
import { useTheme } from '@/components/theme/theme-context'
import {
  Menubar,
  MenubarContent,
  MenubarGroup,
  MenubarItem,
  MenubarLabel,
  MenubarMenu,
  MenubarRadioGroup,
  MenubarRadioItem,
  MenubarSeparator,
  MenubarTrigger,
} from '@/components/ui/menubar'

const appearances = [
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
  { value: 'system', label: 'System', icon: Monitor },
] as const

export function AppMenubar() {
  const { theme, setTheme } = useTheme()
  return (
    <Menubar
      aria-label="Application menu"
      className="h-9 gap-1 border-0 bg-transparent p-0"
    >
      <MenubarMenu>
        <MenubarTrigger className="h-8 rounded-md px-2.5 text-sm">
          Pages
        </MenubarTrigger>
        <MenubarContent>
          {workspacePages.map(({ to, label, icon: Icon }) => (
            <MenubarItem
              key={to}
              className="text-sm font-normal tracking-normal normal-case"
              render={
                <Link
                  to={to}
                  activeOptions={{
                    exact: to !== '/downloads',
                    includeSearch: false,
                  }}
                  search={to === '/downloads' ? {} : undefined}
                />
              }
            >
              <Icon aria-hidden="true" />
              {label}
            </MenubarItem>
          ))}
          <MenubarSeparator />
          <MenubarItem
            className="text-sm font-normal tracking-normal normal-case"
            render={<Link to="/health_check" />}
          >
            <Activity aria-hidden="true" />
            System health
          </MenubarItem>
        </MenubarContent>
      </MenubarMenu>
      <MenubarMenu>
        <MenubarTrigger className="h-8 rounded-md px-2.5 text-sm">
          Appearance
        </MenubarTrigger>
        <MenubarContent>
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
                <MenubarRadioItem
                  key={value}
                  value={value}
                  closeOnClick
                  className="text-sm font-normal tracking-normal normal-case"
                >
                  <Icon aria-hidden="true" />
                  {label}
                </MenubarRadioItem>
              ))}
            </MenubarRadioGroup>
          </MenubarGroup>
        </MenubarContent>
      </MenubarMenu>
    </Menubar>
  )
}
