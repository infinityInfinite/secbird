# Icônes et PNG transparents

Les icônes standard sont utilisées par défaut. Pour ajouter une vraie image :

1. Placer le PNG transparent dans ce dossier, par exemple `basilisk.png`.
2. Dans `ui/device-assets.js`, ajouter `mouse: { png: 'assets/devices/basilisk.png' }` pour un modèle, ou `'pid:00D7': { png: 'assets/devices/basilisk.png' }` pour une variante précise. Le PID est prioritaire.
3. Recompiler l'EXE. Toutes les vues (Dashboard, appareil et profils) utilisent le même registre.


La règle CSS commune `object-fit: contain` préserve le ratio et la transparence. Une ressource introuvable revient automatiquement à l'icône. Souris : `mouse`, clavier : `keyboard`, tapis : `mat`, dock : `dock`, ARGB : `argb`, dongle : `receiver`.
