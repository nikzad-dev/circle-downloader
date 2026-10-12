import {
  Download,
  FolderOpen,
  Globe,
  KeyRound,
  SlidersHorizontal,
} from 'lucide-react'
import { z } from 'zod'

export const settingsSectionSchema = z.enum([
  'general',
  'downloads',
  'folders',
  'network',
  'credentials',
])
export type SettingsSection = z.infer<typeof settingsSectionSchema>
export const settingsSections = [
  {
    id: 'general',
    label: 'General',
    description: 'Appearance and window behavior.',
    icon: SlidersHorizontal,
  },
  {
    id: 'downloads',
    label: 'Transfers',
    description: 'Choose how your downloads run.',
    icon: Download,
  },
  {
    id: 'folders',
    label: 'Folders',
    description: 'Manage approved locations and your default destination.',
    icon: FolderOpen,
  },
  {
    id: 'network',
    label: 'Network',
    description: 'Bandwidth, proxies, and connection timeouts.',
    icon: Globe,
  },
  {
    id: 'credentials',
    label: 'Credentials',
    description: 'Manage authorization for downloads and proxies.',
    icon: KeyRound,
  },
] as const
