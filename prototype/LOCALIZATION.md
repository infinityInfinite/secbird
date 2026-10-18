# Localization

The application starts in **English**, offline, regardless of Windows/browser language. No translation service or runtime dependency is used.

`ui/i18n.js` exposes `I18n.register(code, messages)`, `I18n.t(message, values)`, `I18n.setLocale(code)` and `I18n.apply(root)`. English source messages are dictionary keys. A missing or empty translation falls back to the English message. `ui/locales/en.js` is the default dictionary; `example.js` is an inactive template.

To add a language, copy the template to `ui/locales/fr.js`, fill its dictionary, and load it after `locales/en.js` in `index.html`. The local server serves locale JavaScript from this folder. Call `I18n.setLocale('fr')` to select the registered language. Selection lasts for the session; startup remains English until an explicit language preference is implemented.

Static visible text and accessibility labels are translated after rendering. For interpolated messages, use `I18n.t('Profile {name} saved.', {name})`, and add the same source key to the dictionary. Insert translations with `textContent` or HTML escaping, never as trusted HTML. Use stable, English `value` attributes on translated selects; stored values must remain language independent.

Device names, profile names, avatar text and user input must stay untouched: add `data-i18n-skip` to new user-content containers. Known user-content components already opt out. Window controls/tray strings are currently English Rust strings, separate from the frontend dictionaries; localizing the tray will require passing translated labels through a dedicated desktop command.

Legacy French lighting-effect values are accepted and normalized to English when loading profiles. User-defined profile names and button assignments are preserved.
