// Runs from <head> before the first paint (#4658): puts the saved theme's `dark` class on <html>
// so a dark start never flashes white. A plain file, not an inline script, so the CSP's
// `script-src 'self'` covers it. The choice is mirrored in localStorage by src/lib/theme.ts
// (same key and rule); the Rust `settings` table stays the source of truth.
(function () {
  var theme = null;
  try {
    theme = window.localStorage.getItem("projecttrucks-theme");
  } catch (e) {
    // No storage: fall through to "system".
  }
  var dark =
    theme === "dark" ||
    (theme !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  var root = document.documentElement;
  root.classList.toggle("dark", dark);
  root.style.colorScheme = dark ? "dark" : "light";
})();
