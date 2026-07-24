import type { AuthProviderKind, AuthValidationTarget } from '../types/domain'
import { authValidationResultKey } from './authValidation'

export type AuthValidationJobStatus = 'running' | 'done' | 'error'

export type AuthValidationJob = {
  id: string
  kind: AuthProviderKind
  /** Empty means all providers of this kind. */
  targets: AuthValidationTarget[]
  status: AuthValidationJobStatus
  total: number
  completed: number
  currentLabel: string
  error?: string
}

export function createAuthValidationJobId() {
  return `auth-validate-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

export function jobCoversProvider(job: AuthValidationJob, providerIndex: number) {
  if (job.kind && job.targets.length === 0) return true
  return job.targets.some((target) => target.providerIndex === providerIndex)
}

export function jobCoversTarget(job: AuthValidationJob, target: AuthValidationTarget) {
  if (job.targets.length === 0) return true
  const key = authValidationResultKey(target)
  return job.targets.some((item) => authValidationResultKey(item) === key)
}

export function isProviderValidating(
  jobs: AuthValidationJob[],
  kind: AuthProviderKind,
  providerIndex: number,
) {
  return jobs.some(
    (job) =>
      job.status === 'running' &&
      job.kind === kind &&
      jobCoversProvider(job, providerIndex),
  )
}

export function isTargetValidating(
  jobs: AuthValidationJob[],
  kind: AuthProviderKind,
  target: AuthValidationTarget,
) {
  return jobs.some(
    (job) =>
      job.status === 'running' &&
      job.kind === kind &&
      jobCoversTarget(job, target),
  )
}

export function providerValidationProgress(
  jobs: AuthValidationJob[],
  kind: AuthProviderKind,
  providerIndex: number,
): { completed: number; total: number; label: string } | null {
  const job = jobs.find(
    (item) =>
      item.status === 'running' &&
      item.kind === kind &&
      jobCoversProvider(item, providerIndex),
  )
  if (!job || job.total <= 0) return null
  // When job spans multiple providers, estimate progress for this provider by completed labels
  // using overall job progress (good enough for UX).
  return {
    completed: job.completed,
    total: job.total,
    label: job.currentLabel,
  }
}

export function isKindValidating(jobs: AuthValidationJob[], kind: AuthProviderKind) {
  return jobs.some((job) => job.status === 'running' && job.kind === kind)
}
