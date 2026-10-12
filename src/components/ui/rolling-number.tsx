import type { RollingNumberProps, RollingTextProps } from '@kitlangton/rolling-number/react'
import {
  RollingNumber as BaseRollingNumber,
  RollingText as BaseRollingText,
} from '@kitlangton/rolling-number/react'
import { cn } from 'cn'

export function RollingNumber({ className, ...props }: RollingNumberProps) {
  return (
    <BaseRollingNumber
      className={cn('tabular-nums', className)}
      {...props}
    />
  )
}

export function RollingText({ className, ...props }: RollingTextProps) {
  return (
    <BaseRollingText
      className={cn('tabular-nums', className)}
      {...props}
    />
  )
}

export type { RollingNumberProps, RollingTextProps }
