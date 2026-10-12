// Installed by the local app entry point only, never injected into remote webviews.
export async function handleExternalLinkClick(event, openUrl, onError) {
  if (event.defaultPrevented || event.button !== 0 || event.altKey) return
  const anchor = event.composedPath().find(node => node?.tagName?.toLowerCase() === 'a')
  if (!anchor?.href || anchor.hasAttribute('download')) return
  if (anchor.target !== '_blank' && !event.ctrlKey && !event.metaKey && !event.shiftKey) return

  let url
  try { url = new URL(anchor.href) } catch { return }
  if (!['http:', 'https:', 'mailto:', 'tel:'].includes(url.protocol)) return

  event.preventDefault()
  try { await openUrl(url.href) }
  catch (error) { onError(error) }
}
