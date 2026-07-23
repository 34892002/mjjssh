import { computed, ref } from 'vue'
import { dateEnUS, dateZhCN, enUS, zhCN } from 'naive-ui'
import enUSMessages from '../locales/en-US'
import zhCNMessages from '../locales/zh-CN'

export type AppLanguage = 'zh-CN' | 'en-US'
export type MessageKey = keyof typeof zhCNMessages

type TranslationParams = Record<string, string | number>

const messages = {
  'zh-CN': zhCNMessages,
  'en-US': enUSMessages,
}

const LANGUAGE_STORAGE_KEY = 'my-ssh-language'

function interpolate(template: string, params: TranslationParams) {
  return template.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? `{${name}}`))
}

function detectInitialLanguage(): AppLanguage {
  const storedLanguage = localStorage.getItem(LANGUAGE_STORAGE_KEY)
  if (storedLanguage === 'zh-CN' || storedLanguage === 'en-US') return storedLanguage

  const systemLanguage = (navigator.languages?.[0] ?? navigator.language ?? '').toLowerCase()
  return systemLanguage.startsWith('zh-cn') || systemLanguage.startsWith('zh-hans') ? 'zh-CN' : 'en-US'
}

const language = ref<AppLanguage>(detectInitialLanguage())

document.documentElement.lang = language.value

export function useLocale() {
  function t(key: MessageKey, params: TranslationParams = {}) {
    return interpolate(messages[language.value][key], params)
  }

  const languageLabel = computed(() => language.value === 'zh-CN' ? t('language.zh') : t('language.en'))
  const naiveLocale = computed(() => language.value === 'zh-CN' ? zhCN : enUS)
  const naiveDateLocale = computed(() => language.value === 'zh-CN' ? dateZhCN : dateEnUS)

  function setLanguage(nextLanguage: AppLanguage) {
    language.value = nextLanguage
    localStorage.setItem(LANGUAGE_STORAGE_KEY, nextLanguage)
    document.documentElement.lang = nextLanguage
  }

  return {
    language,
    languageLabel,
    naiveLocale,
    naiveDateLocale,
    setLanguage,
    t,
  }
}
