// Read a CSV chosen in a file picker, strictly as UTF-8.

const MAX_BYTES = 1024 * 1024

export type CsvFile = { ok: true; name: string; text: string } | { ok: false; error: string }

/**
 * Reads the chosen file and clears the picker, so choosing the same (fixed) file again
 * still triggers `change`. Returns `null` when nothing was chosen.
 */
export async function readCsvFile(input: HTMLInputElement): Promise<CsvFile | null> {
  const file = input.files?.[0]
  input.value = ''
  if (!file) return null
  if (file.size > MAX_BYTES) return { ok: false, error: 'That file is larger than 1 MB. Split it into smaller files.' }
  try {
    // `fatal` makes a non-UTF-8 file fail here instead of turning letters into "�".
    const text = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer())
    return { ok: true, name: file.name, text }
  } catch {
    return {
      ok: false,
      error: 'This file isn\'t saved as UTF-8. In Excel choose "Save As" → "CSV UTF-8 (Comma delimited)", then choose it again.',
    }
  }
}
