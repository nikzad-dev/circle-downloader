import { useId } from 'react'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { useTheme } from './theme-context'

export function ThemeSelector() {
  const { theme, setTheme } = useTheme()
  const id = useId()
  return (
    <RadioGroup
      aria-label="Appearance"
      value={theme}
      onValueChange={(value: unknown) => {
        if (value === 'light' || value === 'dark' || value === 'system')
          setTheme(value)
      }}
      className="gap-2"
    >
      {(['light', 'dark', 'system'] as const).map((value) => (
        <Label
          key={value}
          htmlFor={`${id}-${value}`}
          className="flex cursor-pointer items-center gap-3 py-1 text-sm capitalize"
        >
          <RadioGroupItem id={`${id}-${value}`} value={value} />
          {value[0].toUpperCase() + value.slice(1)}
        </Label>
      ))}
    </RadioGroup>
  )
}
