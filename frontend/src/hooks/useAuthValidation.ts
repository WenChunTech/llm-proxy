import { useState } from 'react'
import type { Dispatch, SetStateAction } from 'react'
import { providerMeta } from '../config/providers'
import { apiAuthHeaders } from '../lib/api'
import {
  applyAuthValidationResults,
  buildAuthValidationConfig,
  deleteProviderAuthTarget,
  mergeAuthValidationState,
  setProviderAuthDisabled,
  syncAuthValidationPayloadWithProviders,
  visibleAuthValidationResults,
} from '../lib/authValidation'
import type {
  AuthProviderKind,
  AuthStatus,
  AuthValidationFilter,
  AuthValidationResponse,
  AuthValidationState,
  AuthValidationTarget,
  Provider,
  ProviderKind,
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
  const [validatingAuthKind, setValidatingAuthKind] = useState<ProviderKind | null>(null)
  const [authValidation, setAuthValidation] = useState<AuthValidationState | null>(null)

  async function validateAuths(
    kind: AuthProviderKind,
    validateOptions: { targets?: AuthValidationTarget[]; replace?: boolean } = {},
  ) {
    const kindProviders = providers.filter((provider) => provider.kind === kind)
    if (!kindProviders.length) {
      setToast(`没有可校验的 ${providerMeta[kind].label} 提供商`)
      return
    }
    if (validateOptions.targets && !validateOptions.targets.length) {
      setToast('当前筛选项没有可校验的 auth')
      return
    }
    setValidatingAuthKind(kind)
    if (validateOptions.replace !== false) setAuthValidation(null)
    try {
      const response = await fetch(`/api/${kind}/validate`, {
        method: 'POST',
        headers: apiAuthHeaders(accessKey, { 'content-type': 'application/json' }),
        body: JSON.stringify({
          config: buildAuthValidationConfig(providers),
          ...(validateOptions.targets ? { targets: validateOptions.targets } : {}),
        }),
      })
      if (response.status === 401) {
        setAuthStatus('login')
        throw new Error('unauthorized')
      }
      if (!response.ok) throw new Error(await response.text())
      const payload = (await response.json()) as AuthValidationResponse
      if (!payload.success) throw new Error('validation failed')
      const nextProviders = applyAuthValidationResults(providers, kind, payload.data.results)
      const nextValidation = mergeAuthValidationState(
        validateOptions.replace === false ? authValidation : null,
        kind,
        payload.data,
      )
      setProviders(nextProviders)
      setAuthValidation(nextValidation)
      void persistConfig({ providers: nextProviders })
      setToast(
        `${providerMeta[kind].label} 校验完成：有效 ${nextValidation.payload.valid}，无效 ${nextValidation.payload.invalid}`,
      )
    } catch (error) {
      if (!(error instanceof Error && error.message === 'unauthorized')) {
        setToast(`${providerMeta[kind].label} auth 校验失败`)
      }
    } finally {
      setValidatingAuthKind(null)
    }
  }

  function setAuthValidationFilter(filter: AuthValidationFilter) {
    setAuthValidation((current) => (current ? { ...current, filter } : current))
  }

  function validateAuthTargets(kind: AuthProviderKind, targets?: AuthValidationTarget[]) {
    void validateAuths(kind, targets ? { targets, replace: false } : {})
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
      disabled ? '已禁用 auth，配置保存中' : '已启用 auth，配置保存中',
    )
  }

  function deleteAuthFromValidation(kind: AuthProviderKind, target: AuthValidationTarget) {
    updateAuthValidationProviders(
      kind,
      (currentProviders) => deleteProviderAuthTarget(currentProviders, kind, target),
      '已删除 auth，配置保存中',
    )
  }

  function validateVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation).map((result) => ({
      providerIndex: result.providerIndex,
      authIndex: result.authIndex,
    }))
    void validateAuths(authValidation.kind, { targets, replace: false })
  }

  function disableVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation)
    if (!targets.length) {
      setToast('当前筛选项没有可禁用的 auth')
      return
    }
    updateAuthValidationProviders(
      authValidation.kind,
      (currentProviders) =>
        targets.reduce(
          (nextProviders, result) =>
            setProviderAuthDisabled(nextProviders, authValidation.kind, result, true),
          currentProviders,
        ),
      `已禁用 ${targets.length} 个 auth，配置保存中`,
    )
  }

  function enableVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation)
    if (!targets.length) {
      setToast('当前筛选项没有可启用的 auth')
      return
    }
    updateAuthValidationProviders(
      authValidation.kind,
      (currentProviders) =>
        targets.reduce(
          (nextProviders, result) =>
            setProviderAuthDisabled(nextProviders, authValidation.kind, result, false),
          currentProviders,
        ),
      `已启用 ${targets.length} 个 auth，配置保存中`,
    )
  }

  function deleteVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation)
    if (!targets.length) {
      setToast('当前筛选项没有可删除的 auth')
      return
    }
    const orderedTargets = [...targets].sort(
      (a, b) => b.providerIndex - a.providerIndex || b.authIndex - a.authIndex,
    )
    updateAuthValidationProviders(
      authValidation.kind,
      (currentProviders) =>
        orderedTargets.reduce(
          (nextProviders, result) =>
            deleteProviderAuthTarget(nextProviders, authValidation.kind, result),
          currentProviders,
        ),
      `已删除 ${targets.length} 个 auth，配置保存中`,
    )
  }

  return {
    validatingAuthKind,
    authValidation,
    validateAuths,
    setAuthValidationFilter,
    validateAuthTargets,
    disableAuthFromValidation,
    deleteAuthFromValidation,
    validateVisibleAuthResults,
    disableVisibleAuthResults,
    enableVisibleAuthResults,
    deleteVisibleAuthResults,
  }
}
