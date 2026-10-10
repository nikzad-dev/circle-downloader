import { ArrowDownToLine, Home, Settings } from 'lucide-react'
import { strings } from '@/lib/ui-strings'

export const workspacePages = [
  { to: '/', label: 'Home', icon: Home },
  { to: '/downloads', label: strings.downloads, icon: ArrowDownToLine },
  { to: '/settings', label: strings.settings, icon: Settings },
] as const
