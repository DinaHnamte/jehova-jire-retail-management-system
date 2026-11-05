// Runs before the app so the page never flashes the wrong theme.
// Keep in step with src/shared/theme.ts.
;(function () {
  var t = null
  try {
    t = localStorage.getItem('jj-theme')
  } catch (e) {}
  if (t !== 'light' && t !== 'dark') {
    t = window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
  }
  document.documentElement.dataset.theme = t
})()
