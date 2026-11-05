/** Open a WhatsApp link built by the server. Centralised so tests can mock it. */
export function openWhatsApp(url: string) {
  window.location.href = url
}
