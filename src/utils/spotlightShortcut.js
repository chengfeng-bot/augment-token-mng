export const SPOTLIGHT_SHORTCUT_KEY = 'atm-spotlight-shortcut'
export const SPOTLIGHT_ENABLED_KEY = 'atm-spotlight-enabled'
export const DEFAULT_SPOTLIGHT_SHORTCUT = 'Alt+Space'

export const getSpotlightShortcut = () => localStorage.getItem(SPOTLIGHT_SHORTCUT_KEY)

export const setSpotlightShortcut = (shortcut) => {
  localStorage.setItem(SPOTLIGHT_SHORTCUT_KEY, shortcut)
}

export const isSpotlightEnabled = () => {
  const saved = localStorage.getItem(SPOTLIGHT_ENABLED_KEY)
  if (saved !== null) return saved === 'true'

  const enabled = Boolean(getSpotlightShortcut())
  localStorage.setItem(SPOTLIGHT_ENABLED_KEY, String(enabled))
  return enabled
}

export const setSpotlightEnabled = (enabled) => {
  localStorage.setItem(SPOTLIGHT_ENABLED_KEY, String(enabled))
}
