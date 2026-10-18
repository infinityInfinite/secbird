# Journal de développement

## 2026-10-18 — Mission 1

- Dépôt local inventorié, commit 9f6d0cac305c093ae2f582614e8fed1d48205481. Arbre initial sans modification signalée.
- Audit statique de l'architecture Rust/Win32/HTML, API, transport, profils, capacités, CI et distribution. Aucun outil HID exécuté, aucune commande envoyée, aucun firmware modifié.
- Détection du piège SECBIRD_SIM (appareils réels découverts avant ajout des simulations).
- Compatibilité demandée classée par PID et connexion ; déclaration DB et retour utilisateur distingués des preuves matérielles.
- Ajout de l'exigence EXE/fenêtre autonome, cycle de vie indépendant et mesure incluant WebView2.
- Sources publiques officielles recherchées ; accès aux pages support partiellement bloqué et captures PDF non rendues par l'outil. Fidélité pixel à pixel non vérifiée.
- Complément : guide Basilisk rendu ensuite localement via PDFium, pages Dashboard/Customize/Performance/Lighting examinées ; captures réservées à l'étude, exclues des livrables. Écarts de disposition documentés dans FEASIBILITY.md.
- AUDIT.md et FEASIBILITY.md écrits avant création du prototype. Aucun fichier du backend historique modifié.
- Prototype autonome Tauri 2 et frontend sans bibliothèque UI retenus pour cette phase ; aucune dépendance au core ni à l'API existante.

Les résultats d'exécution et limites sont complétés à la fin de cette entrée.

### Précision de périmètre

L'utilisateur recentre sur le redesign léger et autorise mode web ou fenêtre dédiée Tauri, sans besoin de campagne de tests. La compilation des tests backend déjà engagée a été interrompue ; aucun résultat de non-régression revendiqué. Ajout d'un serveur statique Node optionnel, même UI que la fenêtre native, aucune route matérielle. Les mesures longues ne sont pas exécutées dans ce périmètre révisé.

### Matériel et références utilisateur

- Souris précisée : Phantom Green Edition ; nom de démonstration corrigé.
- Ajout à l'inventaire de Goliathus Chroma, Thunderbolt 4 Dock Chroma et ARGB Controller, avec limites de preuve et absence d'écran de configuration ARGB dans l'amont.
- Dossier `references/synapse4/` créé avec guide de captures ; images ignorées par Git et exclues du frontend.

### Vérifications et livraison

- Quatre tests du modèle de démo déjà réalisés avant recentrage : capacités DPI, isolation des profils, rejet d'un stockage corrompu et absence d'adaptateur matériel : réussis. Vérification syntaxique JS et script PowerShell : réussie.
- Rust stable 1.99.1 téléchargé dans `prototype/.toolchain/` uniquement, sans modification du PATH système. Dépendances de compilation téléchargées ; lockfiles conservés.
- `npm run build` : compilation Tauri Windows release réussie. EXE dans `prototype/src-tauri/target/release/secbird-desktop-demo.exe`. Backend original inchangé.
- WebView2 détecté : 151.0.4129.59 ; outils C++ Visual Studio présents. L'EXE seul requiert le runtime ; l'installateur NSIS hors ligne est configuré mais n'a pas été construit.
- Pas de campagne de tests native/backend ni de benchmark, conformément au recentrage. Aucun chiffre RAM/CPU ni compatibilité matérielle validée annoncé. Script de mesure disponible pour plus tard.

## 2026-10-18 — HOME à partir des captures utilisateur

- Consultation des quatre PNG dans references/synapse4/HOME : dashboard_all_devices, gamer_room, devices_&_modules et global_shortcut. Aucun fichier d'explications texte séparé trouvé dans ce dossier ; les captures ont servi de référence visuelle.
- HOME : remplacement du rail latéral par les bandeaux supérieurs de la référence ; pilule verte active, barre d'historique, outils, onglet d'application et barre de titre personnalisée.
- Dashboard : grille quatre colonnes à 1280 px, cartes 220 px, cinq appareils et dongle distinct, modules à disques verts, sections repliables, grilles adaptées aux fenêtres plus étroites.
- Ajout des vues Gamer Room (scène CSS originale et emplacements vides), Devices & Modules (listes et indisponibilités explicites) et Global Shortcuts (création/suppression d'une liste locale, aucun hook clavier).
- Illustrations produit en SVG et pictogrammes originaux. Aucune capture utilisateur ni image Razer recadrée/extraite intégrée aux assets. Mention DEMO persistante ; aucune compatibilité ou valeur de batterie réelle déduite des captures.
- Pages souris/clavier existantes conservées, accessibles depuis Dashboard ; autres appareils en prévisualisation uniquement. Moteur, protocoles et profils matériels inchangés.
- Fenêtre Tauri sans décorations système : permissions limitées ajoutées pour réduire, agrandir/restaurer, fermer et glisser la fenêtre. Cycle tray conservé.
- Revue visuelle ponctuelle à 1280×1154 et 820×600 via Edge local : quatre vues rendues, navigation et création de raccourci local vérifiées, page souris/Performance encore accessible, pas d'erreur JS ni de débordement horizontal observé. Pas de campagne backend/matériel.
- Reconstruction release Tauri réussie après cette modification : `prototype/src-tauri/target/release/secbird-desktop-demo.exe`. Aucun benchmark ni test de cycle de vie Windows complet effectué.

## 2026-10-18 — Correction d'identité visuelle demandée par l'utilisateur

- Correction de l'interprétation : les captures Synapse servent de référence de disposition et d'ergonomie, pas de palette ni de nouvelle identité pour le fork.
- Reprise de la palette Bridge de l'éditeur Secbird amont : #050505 / #0d0d0e / #131314, accent #ff5412 et valeurs #ffb85d. Logo original Secretarybird réutilisé sans modification. Accents verts de l'interface remplacés.
- Suppression des rendus SVG de produits et de la scène Gamer Room dessinée. Photos stock temporaires de souris et clavier Pexels, stockées localement ; mentions « visuel temporaire », crédits et provenance dans ui/assets/CREDITS.md. Pas de retouche/détourage ni d'image générée.
- Appareils sans ressource photo : emplacements « visuel à venir », aucune nouvelle illustration produit. Les pages de configuration utilisent également les photos temporaires.
- Revue visuelle HOME relancée, navigation préservée. Aucun changement matériel.
- Cette correction remplace les choix de palette et d'illustration décrits dans les entrées précédentes.

## 2026-10-18 — Migration UI et composants réutilisables

- Périmètre utilisateur : poursuivre la migration visuelle de tous les appareils, conserver l'identité Secbird et construire une base réutilisable. Aucun raccordement HID autorisé par cette étape de démonstration.
- Retrait des photos stock et de la barre rapide (historique/refresh, raccourci modules redondant, avatar utilisateur). Navigation nommée HOME / Profils locaux / Paramètres ; icône de profils en couches superposées.
- Icônes simples par défaut, mécanisme partagé de PNG transparents : manifeste, import local, reset et repli automatique. Suppression des JPEG ; aucun dessin produit ni détourage ajouté.
- Registre unique `device-registry.json` avec types, modèles, capacités, pages et options. Génération du script browser avant build ; même JSON embarqué par Rust pour construire le menu tray. Ajout de modèle propagé à Dashboard, profils, pages, visuels et tray.
- Composants partagés `components.js` et renderers par type de page. DPI/polling paramétrés, ports ARGB configurables dans la description, pages conditionnées par capacités.
- Migration des vues des cinq appareils : Customize/Performance/Lighting/Power/Calibration pour la souris, Customize/Lighting/Power pour le clavier, Lighting tapis, Audio/Lighting dock, Customize/Lighting ARGB. Fonctions non intégrées désactivées avec motif. Réglages interactifs clairement simulés et sans matériel.
- Profils regroupés dans un espace dédié : création, duplication, renommage, activation locale, suppression, export/import JSON et associations locales. Aucun sélecteur de profil dans les pages périphériques, seulement un rappel du profil actif et un lien vers la gestion centrale.
- Migration additive du stockage de la démo deux appareils vers cinq : préservation des profils valides existants. Aucun accès au Store Secbird réel.
- Revue ponctuelle : parcours des onglets des cinq modèles, profils, import/reset PNG et restauration après reload, migration des anciennes données, affichage 820 px ; réussie sans accès matériel. Documentation d'extension dans `prototype/COMPONENTS.md`.
- Vérification d'extensibilité : modèle souris supplémentaire injecté uniquement dans le catalogue de la revue UI, sans modification des pages. Carte, onglets (limite DPI 16 000), profils et import PNG apparus automatiquement. Aucun appareil fictif supplémentaire conservé dans le catalogue livré.
- Les pages déclarées `status: unavailable` sont aussi encapsulées dans un groupe de contrôles désactivé, en plus des motifs visibles. Le statut ne dépend pas seulement du style graphique.
- Compilation release du catalogue partagé et du menu tray déclaratif réussie, puis reconstruction finale réussie avec la désactivation générique des pages indisponibles. Vérifications syntaxiques des scripts UI réussies ; quatre tests existants du modèle local réussis. Aucune campagne matérielle/backend.

## 2026-10-18 — Correction de barre et périphériques réels

- Correction du périmètre : barre supérieure rétablie, seul le bouton six points est retiré. Profils locaux accessibles par l'icône en couches ; paramètres et avatar restent à droite. Précédent/suivant et actualisation rétablis.
- Avatar local personnalisable : texte (3 caractères) ou image PNG/JPEG/WebP, stockage local, reset ; aucun compte. Pas de bouton Profils déplacé dans les onglets de titre.
- Remplacement des appareils du catalogue de démonstration par une énumération passive Windows des interfaces HID VID 1532. Réutilisation de `secbird-core::Db` et de son hidapi ; aucun appel à core::discover, aucune requête feature report, aucun SET ni application de profil.
- Regroupement par PID et Container ID Windows (repli serial, puis PID si les propriétés ne sont pas accessibles). Les métadonnées de la base permettent de nommer les appareils et choisir un composant ; elles ne valent pas preuve de compatibilité.
- Inventaire réel vérifié par l'EXE `--list-devices` : 00D7 Phantom Green wireless (14 interfaces), 0258 BlackWidow USB (11), 0C01 Goliathus (7), 0F1F ARGB (3), 0F21 Thunderbolt Dock (1). Cinq cartes physiques, pas de récepteur fictif ajouté. Pour une liaison radio, seul le récepteur USB est prouvé par l'énumération.
- Aucune batterie inventée. En cas d'absence/d'erreur : liste vide et raison, jamais un retour aux modèles de démonstration. Scan au lancement et bouton actualiser, menu tray alimenté par le résultat réel.
- Mode web : GET local /api/devices exécute le même EXE en mode énumération uniquement. Sans EXE Windows ou accès matériel, erreur explicite ; ouverture file:// ne simule pas des périphériques.
- Profils locaux séparés des anciennes données fictives et indexés par identité détectée. Seul Default est proposé initialement ; anciens exemples Gaming/Travail non ajoutés. Tous les réglages matériels restent en lecture seule : leurs valeurs ne sont pas lues ni appliquées, brouillons locaux distincts des profils embarqués.
- Vérification UI sur l'API réellement exécutée sur ce PC : cinq modèles ci-dessus, barre/absence six points, avatar texte/image et persistance, absence de batteries fictives, réponse vide sans fallback démo, pages de réglages désactivées, historique et largeur 820 px. Quatre tests du modèle/absence de commandes de réglage réussis.
- Reconstruction release finale réussie. Vérification également dans la vraie fenêtre Tauri/WebView2 : cinq cartes issues de la commande IPC native, trois boutons dans la barre droite (profils, paramètres, avatar), aucune batterie fictive. Capture `prototype/artifacts/native-dashboard.png`. Instance QA et serveur temporaire arrêtés après vérification.

### Alignement de la barre

- Boutons profils/paramètres/avatar et historique sur des emplacements identiques 30×30 px, sans marges spécifiques de l'avatar ; espacement constant 8 px. Avatar visuel conservé à 22 px à l'intérieur de son bouton.
- Vérification des rectangles rendus : centres à équidistance (38 px) et sur la même ligne. Capture `prototype/artifacts/toolbar.png`.

### Texte de l'avatar

- Texte/avatar placé dans un cercle interne de 22 px, bouton externe conservé à 30 px. Taille du texte ajustée à sa largeur réelle pour éviter la coupure des majuscules, sans modifier l'espacement de la barre.
- Vérification des libellés DAM, WWW, MMM et S : largeur du texte limitée à 20 px.

### Anglais par défaut et ressources développeur

- Interface et tray en anglais, sans changement du thème Secbird. Module hors ligne `prototype/ui/i18n.js`, dictionnaires `ui/locales/`, repli anglais, interpolation et documentation `prototype/LOCALIZATION.md`. Changement de langue réversible dans la session ; aucune langue système imposée.
- Anciennes valeurs d’effets françaises acceptées puis normalisées au chargement ; noms de profils et affectations personnalisées conservés.
- Suppression de l’import produit côté utilisateur. Les développeurs définissent les PNG transparents dans `ui/device-assets.js`, par modèle ou PID ; les vues partagent cette résolution et reviennent à l’icône en cas d’image absente. Avatar utilisateur toujours modifiable.
- Vérification : quatre tests existants réussis ; parcours UI avec cinq appareils réellement détectés, anglais au démarrage, traduction d’exemple et retour anglais, import d’image réservé à l’avatar, absence de faux appareils, contrôles matériels désactivés.

### Publication de la proposition

- Préparation d’une branche de contribution sur le fork x225franc/secbird et d’une PR vers infinityInfinite/secbird, sans modification des crates existantes.
- README du prototype réécrit en anglais pour les mainteneurs, avec comportement actuel, instructions de lancement, frontières matérielles et limites non mesurées.
- Captures Synapse conservées uniquement localement ; binaires, outils locaux, schémas générés et anciennes captures QA exclus de la contribution. Deux captures actuelles du prototype conservées pour la revue visuelle.
