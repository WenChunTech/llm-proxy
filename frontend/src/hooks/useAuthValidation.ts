import { useCallback, useRef, useState } from 'react'
import type { Dispatch, SetStateAction } from 'react'
import { providerMeta } from '../config/providers'
import { t } from '../lib/i18n'
import {
  applyAuthValidationResults,
  deleteProviderAuthTarget,
  mergeAuthValidationState,
  setProviderAuthDisabled,
  syncAuthValidationPayloadWithProviders,
  visibleAuthValidationResults,
} from '../lib/authValidation'
import {
  createAuthValidationJobId,
  isKindValidating,
  isProviderValidating,
  isTargetValidating,
  providerValidationProgress,
  type AuthValidationJob,
} from '../lib/authValidationProgress'
import { streamAuthValidation } from '../lib/authValidationStream'
import {
  persistAuthValidationConcurrency,
  readStoredAuthValidationConcurrency,
} from '../lib/storage'
import type {
  AuthProviderKind,
  AuthStatus,
  AuthValidationFilter,
  AuthValidationState,
  AuthValidationTarget,
  Provider,
} from '../types/domain'

export function useAuthValidation(options: {
  providers: Provider[]
  setProviders: Dispatch<SetStateAction<Provider[]>>
  accessKey: string
  setAuthStatus: (status: AuthStatus) => void
  persistConfig: (next: { providers?: Provider[] }) => Promise<void> | void
  setToast: (message: string) => void
}) {
  const {
    providers,
    setProviders,
    accessKey,
    setAuthStatus,
    persistConfig,
    setToast,
  } = options

  const [authValidation, setAuthValidation] = useState<AuthValidationState | null>(null)
  const [validationJobs, setValidationJobs] = useState<AuthValidationJob[]>([])
  const [validationConcurrency, setValidationConcurrencyState] = useState(() =>
    readStoredAuthValidationConcurrency(),
  )
  const providersRef = useRef(providers)
  providersRef.current = providers
  const authValidationRef = useRef(authValidation)
  authValidationRef.current = authValidation
  const jobsRef = useRef(validationJobs)
  jobsRef.current = validationJobs
  const abortByJobRef = useRef(new Map<string, () => void>())

  const upsertJob = useCallback((jobId: string, patch: Partial<AuthValidationJob>) => {
    setValidationJobs((current) =>
      current.map((job) => (job.id === jobId ? { ...job, ...patch } : job)),
    )
  }, [])

  const removeJobLater = useCallback((jobId: string) => {
    window.setTimeout(() => {
      setValidationJobs((current) => current.filter((job) => job.id !== jobId))
      abortByJobRef.current.delete(jobId)
    }, 1200)
  }, [])

  const validateAuths = useCallback(
    (
      kind: AuthProviderKind,
      validateOptions: { targets?: AuthValidationTarget[]; replace?: boolean } = {},
    ) => {
      const kindProviders = providersRef.current.filter((provider) => provider.kind === kind)
      if (!kindProviders.length) {
        setToast(t('toast.noValidatable', { label: providerMeta[kind].label }))
        return
      }
      const targets = validateOptions.targets
      if (targets && !targets.length) {
        setToast(t('toast.noAuthToValidate'))
        return
      }

      const currentJobs = jobsRef.current
      const blocked = targets?.length
        ? targets.some((target) => isTargetValidating(currentJobs, kind, target))
        : isKindValidating(currentJobs, kind)
      if (blocked) {
        setToast(t('toast.alreadyValidating', { label: providerMeta[kind].label }))
        return
      }

      const jobId = createAuthValidationJobId()
      const job: AuthValidationJob = {
        id: jobId,
        kind,
        targets: targets ? [...targets] : [],
        status: 'running',
        total: 0,
        completed: 0,
        currentLabel: t('toast.connecting'),
      }
      setValidationJobs((current) => [...current, job])

      if (validateOptions.replace !== false && !targets?.length) {
        setAuthValidation(null)
      }

      const abort = streamAuthValidation({
        kind,
        accessKey,
        providers: providersRef.current,
        targets,
        concurrency: validationConcurrency,
        handlers: {
          onStarted: (event) => {
            upsertJob(jobId, {
              total: event.total,
              completed: 0,
              currentLabel: event.total > 0 ? `0/${event.total}` : t('toast.preparing'),
            })
          },
          onResult: (event) => {
            upsertJob(jobId, {
              total: event.total,
              completed: event.completed,
              currentLabel: event.result.label || `${event.completed}/${event.total}`,
            })
            setProviders((currentProviders) => {
              const nextProviders = applyAuthValidationResults(
                currentProviders,
                kind,
                [event.result],
              )
              providersRef.current = nextProviders
              return nextProviders
            })
            setAuthValidation((current) =>
              mergeAuthValidationState(current, kind, {
                model: '',
                total: 1,
                checked: event.result.skipped ? 0 : 1,
                valid: event.result.valid && event.result.reason !== 'rate_limited' ? 1 : 0,
                invalid: !event.result.valid && !event.result.skipped ? 1 : 0,
                skipped: event.result.skipped ? 1 : 0,
                rateLimited: event.result.reason === 'rate_limited' ? 1 : 0,
                refreshed: event.result.refreshed ? 1 : 0,
                results: [event.result],
              }),
            )
          },
          onDone: (event) => {
            // Prefer the latest React state snapshot, then fall back to the ref.
            // Always re-apply the full result set so multi-auth arrays cannot shrink.
            const baseProviders = providersRef.current
            const nextProviders = applyAuthValidationResults(
              baseProviders,
              kind,
              event.data.results,
            )
            providersRef.current = nextProviders
            setProviders(nextProviders)
            const nextValidation = mergeAuthValidationState(
              // For targeted revalidation keep previous panel state base.
              targets?.length ? authValidationRef.current : authValidationRef.current,
              kind,
              event.data,
            )
            setAuthValidation(nextValidation)
            void persistConfig({ providers: nextProviders })
            upsertJob(jobId, {
              status: 'done',
              total: event.data.total,
              completed: event.data.total,
              currentLabel: t('toast.validationDone'),
            })
            setToast(
              t('toast.validationSummary', { label: providerMeta[kind].label, valid: nextValidation.payload.valid, invalid: nextValidation.payload.invalid }),
            )
            removeJobLater(jobId)
          },
          onError: (message) => {
            if (message.toLowerCase().includes('unauthorized') || message.includes('401')) {
              setAuthStatus('login')
            } else {
              setToast(
                message && message !== t('stream.wsFailed') && message !== t('stream.wsDisconnected')
                  ? t('toast.validationFailedDetail', { label: providerMeta[kind].label, message })
                  : t('toast.validationFailed', { label: providerMeta[kind].label }),
              )
            }
            upsertJob(jobId, {
              status: 'error',
              error: message,
              currentLabel: t('toast.failed'),
            })
            removeJobLater(jobId)
          },
        },
      })

      abortByJobRef.current.set(jobId, abort)
    },
    [
      accessKey,
      persistConfig,
      removeJobLater,
      setAuthStatus,
      setProviders,
      setToast,
      upsertJob,
      validationConcurrency,
    ],
  )

  function setAuthValidationFilter(filter: AuthValidationFilter) {
    setAuthValidation((current) => (current ? { ...current, filter } : current))
  }

  function validateAuthTargets(kind: AuthProviderKind, targets?: AuthValidationTarget[]) {
    validateAuths(kind, targets ? { targets, replace: false } : { replace: true })
  }

  function updateAuthValidationProviders(
    kind: AuthProviderKind,
    updater: (providers: Provider[]) => Provider[],
    message: string,
  ) {
    const nextProviders = updater(providers)
    setProviders(nextProviders)
    setAuthValidation((current) =>
      current?.kind === kind
        ? {
            ...current,
            payload: syncAuthValidationPayloadWithProviders(current.payload, nextProviders, kind),
          }
        : current,
    )
    void persistConfig({ providers: nextProviders })
    setToast(message)
  }

  function disableAuthFromValidation(
    kind: AuthProviderKind,
    target: AuthValidationTarget,
    disabled: boolean,
  ) {
    updateAuthValidationProviders(
      kind,
      (currentProviders) => setProviderAuthDisabled(currentProviders, kind, target, disabled),
      disabled ? t('toast.authDisabled') : t('toast.authEnabled'),
    )
  }

  function deleteAuthFromValidation(kind: AuthProviderKind, target: AuthValidationTarget) {
    updateAuthValidationProviders(
      kind,
      (currentProviders) => deleteProviderAuthTarget(currentProviders, kind, target),
      t('toast.authDeleted'),
    )
  }

  function validateVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation).map((result) => ({
      providerIndex: result.providerIndex,
      authIndex: result.authIndex,
    }))
    validateAuths(authValidation.kind, { targets, replace: false })
  }

  function enableVisibleAuthResults() {
    if (!authValidation) return
    const kind = authValidation.kind
    const targets = visibleAuthValidationResults(authValidation)
    updateAuthValidationProviders(
      kind,
      (currentProviders) =>
        targets.reduce(
          (next, result) =>
            setProviderAuthDisabled(
              next,
              kind,
              { providerIndex: result.providerIndex, authIndex: result.authIndex },
              false,
            ),
          currentProviders,
        ),
      t('toast.enabledCount', { count: targets.length }),
    )
  }

  function disableVisibleAuthResults() {
    if (!authValidation) return
    const kind = authValidation.kind
    const targets = visibleAuthValidationResults(authValidation)
    updateAuthValidationProviders(
      kind,
      (currentProviders) =>
        targets.reduce(
          (next, result) =>
            setProviderAuthDisabled(
              next,
              kind,
              { providerIndex: result.providerIndex, authIndex: result.authIndex },
              true,
            ),
          currentProviders,
        ),
      t('toast.disabledCount', { count: targets.length }),
    )
  }

  function deleteVisibleAuthResults() {
    if (!authValidation) return
    const kind = authValidation.kind
    const targets = visibleAuthValidationResults(authValidation)
    updateAuthValidationProviders(
      kind,
      (currentProviders) =>
        [...targets]
          .sort((a, b) => b.authIndex - a.authIndex || b.providerIndex - a.providerIndex)
          .reduce(
            (next, result) =>
              deleteProviderAuthTarget(next, kind, {
                providerIndex: result.providerIndex,
                authIndex: result.authIndex,
              }),
            currentProviders,
          ),
      t('toast.deletedCount', { count: targets.length }),
    )
  }

  function setValidationConcurrency(value: number) {
    const next = persistAuthValidationConcurrency(value)
    setValidationConcurrencyState(next)
  }

  function clearAuthValidation() {
    setAuthValidation(null)
  }

  return {
    authValidation,
    validationJobs,
    validationConcurrency,
    setValidationConcurrency,
    clearAuthValidation,
    isKindValidating: (kind: AuthProviderKind) => isKindValidating(validationJobs, kind),
    isProviderValidating: (kind: AuthProviderKind, providerIndex: number) =>
      isProviderValidating(validationJobs, kind, providerIndex),
    isTargetValidating: (kind: AuthProviderKind, target: AuthValidationTarget) =>
      isTargetValidating(validationJobs, kind, target),
    providerValidationProgress: (kind: AuthProviderKind, providerIndex: number) =>
      providerValidationProgress(validationJobs, kind, providerIndex),
    validateAuths,
    validateAuthTargets,
    setAuthValidationFilter,
    validateVisibleAuthResults,
    enableVisibleAuthResults,
    disableVisibleAuthResults,
    deleteVisibleAuthResults,
    disableAuthFromValidation,
    deleteAuthFromValidation,
  }
}
