# Étendre l'interface Secbird

La source de vérité est `ui/device-registry.json`. Elle est partagée par le frontend et le menu tray Rust. Le fichier `device-registry.js` est généré : ne pas le modifier directement.

**Mise à jour : le JSON décrit les composants des modèles, pas les appareils présents.** Les cartes et le tray proviennent désormais uniquement de `src-tauri/src/discovery.rs`. Le frontend fusionne ces résultats dans `registry.js` à l'aide des PID. Une entrée de modèle se rattache au matériel via `"pids": ["00D6", "00D7"]` par exemple. Ajouter un modèle au JSON ne crée donc jamais une carte d'appareil débranché. Un modèle non connu mais détecté reçoit une vue générique et son nom issu des descripteurs/DB. Les premiers paragraphes décrivant un tray alimenté directement par des exemples sont historiques.

`discovery.js` appelle la commande Tauri `detect_devices` ; le mode web utilise l'EXE local `--list-devices`. La découverte réutilise la DB de Secbird et l'énumération hidapi, sans `core::discover`, sans rapport GET/SET de protocole. Battery et réglages actuels ne sont pas lus. Les composants des pages détectées sont encapsulés en lecture seule, même si un renderer sait construire un contrôle interactif. Le backend historique de réglages reste inchangé.

`identity.js` est un composant séparé pour l'avatar texte/image persisté. Il ne représente pas un compte. La barre de navigation et les accès profils/paramètres restent visibles ; seul le raccourci six points a été retiré.

## Ajouter un modèle d'un type existant

Ajouter une entrée dans `devices`, puis lancer `npm run prepare:ui` pour le mode web, ou `npm run build` pour l'EXE (génération automatique). Exemple :

```json
{
  "id": "nouvelle-souris",
  "name": "Nouvelle souris",
  "cardName": "NOUVELLE SOURIS",
  "type": "mouse",
  "connection": "USB · simulé",
  "configurable": true,
  "capabilities": {
    "dpi": { "min": 100, "max": 16000, "step": 100 },
    "polling": [125, 500, 1000],
    "lighting": true
  },
  "visual": { "icon": "mouse", "png": null }
}
```

Dashboard, onglets, espace profils, menu tray utilisent cette déclaration. Aucun ajout d'ID dans un switch de page n'est nécessaire. Les profils d'un nouveau modèle sont créés automatiquement, sans effacer ceux des anciens appareils. Les limites des DPI/polling proviennent de ses capacités.

Les valeurs et capacités de ce catalogue décrivent uniquement le prototype. Elles ne prouvent pas le support matériel. **Aucune commande HID ne peut partir de ce frontend.** La future API matérielle devra fournir un registre validé par PID, transport et firmware, puis refuser les capacités inconnues.

## Composer les pages

Les types (`mouse`, `keyboard`, `mousepad`, `dock`, `argb-controller`, `receiver`) fournissent des pages par défaut. Une entrée appareil peut remplacer `pages` pour choisir son assemblage :

```json
"pages": [
  { "label": "Lighting", "view": "lighting", "requires": "lighting", "options": { "hero": true } },
  { "label": "Customize", "view": "ports", "status": "unavailable", "options": { "ports": 4, "maxLeds": 60 } }
]
```

`requires` masque une page si la capacité correspondante est absente. `options` adapte la disposition d'un composant sans le recopier. Une fonction non intégrée conserve des contrôles désactivés et une raison visible. `status` documente aussi ce fait au niveau de la page ; il ne peut pas activer une fonction matérielle.

Une fonction nouvelle nécessite un renderer dans `devices.js` et l'ajout de son nom à la validation de `sync-device-registry.cjs`. Ensuite, tous les modèles peuvent la réutiliser. Ne pas copier une page complète pour un autre modèle.

## Composants communs

| Fichier | Responsabilité |
|---|---|
| `device-registry.json` / `registry.js` | Types, appareils, fusion des options, capacités et pages |
| `components.js` | `panel`, `unavailable`, `slider`, `toggle`, `columns`, `stage`, `deviceCard`, `deviceTabs` |
| `visuals.js` | Icônes, affichage PNG, repli automatique, configuration développeur |
| `device-assets.js` | Overrides visuels optionnels ; sinon les visuels du catalogue font foi |
| `devices.js` | Renderers partagés : sensibilité, éclairage, alimentation, clavier, audio, ports et calibration |
| `model.js` | Profils de démonstration par appareil, validation, migration additive et limites DPI |
| `app.js` | Navigation, espace profils, import/export, préférences et orchestration |
| `home.js` | Composition des vues HOME à partir du même catalogue |

Ces composants sont des fonctions de rendu HTML/SVG simples. Aucun framework UI ou service distant n'est ajouté. Le thème est commun : les vues ne redéfinissent pas l'identité d'un modèle.

## Remplacer une icône par un PNG

Mettre un PNG transparent dans `ui/assets/devices/` et définir `visual.png`, par exemple `assets/devices/basilisk.png`. Toutes les vues changent ensemble. Le serveur web local accepte ces noms de fichiers sans modification de ses routes. Le ratio et le canal alpha sont conservés ; image absente → icône.


## État fonctionnel

**Local et utilisable :** navigation, profils CRUD/duplication/import-export JSON, associations sauvegardées, configuration développeur des PNG, préférences de fenêtre Tauri.

**Simulation :** DPI, polling, affectations de souris, luminosité/effets, veille souris. Les valeurs restent dans les profils de démonstration.

**Non intégré, contrôles désactivés :** remapping clavier, Snap Tap/mode jeu, molette avancée, pairage, audio Windows, calibration, mémoire embarquée, configuration/détection des ports ARGB, automatismes d'éclairage et macros dans le nouveau frontend. Le moteur historique n'est pas remplacé ; ses fonctions restent dans l'ancienne application.

## Langue et ressources produit (état actuel)

L’anglais est la langue source. Le module de traduction et ses dictionnaires sont documentés dans LOCALIZATION.md. Les ressources produit se configurent uniquement côté développeur dans ui/device-assets.js, par modèle ou PID ; aucun import produit utilisateur. L’avatar local conserve sa personnalisation texte/image.
