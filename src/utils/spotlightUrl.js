export const getWebsiteUrl = (query) => {
  const input = query.trim()
  if (!input || /[\s\\\u0000-\u001f\u007f]/u.test(input)) return null

  const hasProtocol = /^https?:\/\//i.test(input)
  const address = hasProtocol ? input.replace(/^https?:\/\//i, '') : input
  const authority = address.split(/[/?#]/, 1)[0]
  if (!authority || authority.includes('@') || (!hasProtocol && /^[a-z][a-z0-9+.-]*:\/\//i.test(input))) return null

  try {
    const url = new URL(hasProtocol ? input : `https://${input}`)
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) return null
    if (!hasProtocol) {
      const hostname = url.hostname
      const labels = hostname.split('.')
      const isIpv4 = /^\d+\.\d+\.\d+\.\d+$/.test(authority.split(':')[0])
      const isIpv6 = hostname.startsWith('[') && hostname.endsWith(']')
      const isDomain = labels.length > 1 && hostname.length <= 253
        && labels.every(label => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/i.test(label))
        && /^(?:[a-z]{2,}|xn--[a-z0-9-]+)$/i.test(labels.at(-1))
      if (!isDomain && !isIpv4 && !isIpv6) return null
    }
    return url.href
  } catch {
    return null
  }
}
