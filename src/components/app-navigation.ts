import {
  ArrowDownToLine,
  ChartNoAxesCombined,
  Home,
  ListOrdered,
  Settings,
} from 'lucide-react'
import { strings } from '@/lib/ui-strings'

export const workspacePages = [
  { to: '/overview', label: 'Overview', icon: Home },
  { to: '/downloads', label: strings.downloads, icon: ArrowDownToLine },
  { to: '/queues', label: 'Queues', icon: ListOrdered },
  { to: '/statistics', label: 'Statistics', icon: ChartNoAxesCombined },
  { to: '/settings', label: strings.settings, icon: Settings },
] as const
