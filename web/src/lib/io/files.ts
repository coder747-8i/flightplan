// Saving and opening files. In the desktop app saving goes through the native Save dialog
// (some webviews ignore blob downloads); in a browser it is an ordinary download.
import { api } from '../api'
import { config } from '../stores/config.svelte'

/** Returns where the file went (desktop), 'downloaded' (browser) or null if cancelled. */
export async function saveFile(name: string, content: string | Blob, type: string, label: string): Promise<string | null> {
  const blob = typeof content === 'string' ? new Blob([content], { type }) : content
  if (config.desktop) {
    const r = await api.hostSave(name, label, blob)
    return r.cancelled ? null : (r.path ?? name)
  }
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = name
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 10_000)
  return 'downloaded'
}

/** Let the user choose one or more files. Resolves to [] when they cancel. */
export function pickFiles(accept: string, multiple = false): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = accept
    input.multiple = multiple
    input.style.display = 'none'
    input.onchange = () => {
      resolve(Array.from(input.files ?? []))
      input.remove()
    }
    // fires in current browsers when the dialog is dismissed
    input.addEventListener('cancel', () => {
      resolve([])
      input.remove()
    })
    document.body.appendChild(input)
    input.click()
  })
}

export function openExternal(url: string) {
  // mail links hand over to the mail app without leaving a blank tab behind
  if (url.startsWith('mailto:')) window.location.href = url
  else if (config.desktop) api.hostOpen(url).catch(() => window.open(url, '_blank', 'noopener'))
  else window.open(url, '_blank', 'noopener')
}

export function printPage() {
  if (config.desktop) api.hostPrint().catch(() => window.print())
  else window.print()
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    // http:// on a LAN address has no clipboard API; fall back to a hidden textarea
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    const ok = document.execCommand('copy')
    ta.remove()
    return ok
  }
}

/** file-name-safe text */
export const slug = (s: string) => s.replace(/[^\w.-]+/g, '-').replace(/^-+|-+$/g, '') || 'untitled'
