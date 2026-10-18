# Feuille de route

Statuts : 🟢 réalisé, 🟡 partiel / validation requise, 🔴 à faire.

| Phase | Statut | Critère de sortie |
|---|---|---|
| Inventaire et audit statique | 🟢 | AUDIT.md, commit de référence, frontières et risques documentés |
| Preuves matérielles USB / HyperSpeed | 🔴 | rapports par PID/firmware/transport, aucune compatibilité extrapolée |
| Référence Synapse 4 | 🟡 | sources publiques identifiées ; captures exactes et comparaison visuelle encore requises |
| Démo UI isolée | 🟡 | Dashboard, appareils, onglets, DPI et profils ; validation desktop requise |
| Hôte Tauri 2 Windows | 🟡 | EXE release compilé ; fenêtre/tray/instance/auto-start à valider à l'usage |
| Benchmark RAM/CPU total | 🔴 | 3 états, parent + tous WebView2, release, mesures répétées et baseline |
| Contrat API/capacités | 🟡 | architecture documentée ; DTO typés et refus côté Rust à implémenter |
| Intégration moteur existant | 🔴 | extraire Engine, conserver Store/CLI, cycle de vie séparé, tests de non-régression |
| Fonctionnement strictement hors ligne | 🟡 | démo sans réseau ; adapter politique updates/firmware et installation WebView2 du fork |
| Fidélité graphique finale | 🔴 | assets originaux validés, états et captures comparées, tests multi-DPI/accessibilité |
| Redistribution | 🔴 | licences transitives, provenance amont, crédits, signature, installateur EXE hors ligne |

Ordre : audit → démo sans matériel → validation desktop et ressources → API/capacités → intégration progressive par fonction prouvée. Aucun remplacement de l'éditeur existant avant validation.

Périmètre recentré par l'utilisateur : priorité au redesign, mode web ou Tauri accepté, pas de campagne de tests backend/matériels demandée. La même interface est disponible dans les deux modes. Captures utilisateur attendues dans `references/synapse4/` pour la fidélité visuelle. Inventaire réel : Phantom Green Edition, BlackWidow V3 Mini, Goliathus, Thunderbolt Dock et ARGB Controller ; la démo initiale représente les deux premiers.

### Livraison HOME

- 🟢 Captures HOME utilisateur reçues et examinées.
- 🟢 Dashboard, Gamer Room, Devices & Modules, Global Shortcuts reproduits dans leur structure, proportions et navigation ; illustrations originales et données fictives.
- 🟢 Grille des cinq appareils et récepteur séparé, sections repliables, raccourcis locaux et historique de navigation.
- 🟡 Assets produit et scène Gamer Room : illustrations originales indicatives ; pas de reproduction photographique exacte.
- 🔴 Reprise visuelle des pages périphériques : prochaine étape, hors de la livraison HOME.

Correction de direction : 🟢 palette Bridge et logo Secbird conservés ; 🟢 dessins produits remplacés par photos stock souris/clavier temporaires ; 🟡 vraies ressources produit encore attendues. La fidélité Synapse concerne la disposition, pas le vert Razer.

### Migration en composants

- 🟢 Photos retirées : icônes simples et remplacement centralisé par PNG transparent.
- 🟢 Barre rapide/refresh/avatar/modules redondants supprimés, profils centralisés.
- 🟢 Registre unique, composants de cartes/panneaux/onglets/contrôles, renderers réutilisables et tray déclaratif.
- 🟢 Migration visuelle des cinq appareils et de leurs onglets de référence ; fonctions indisponibles désactivées avec motif.
- 🟢 Profils locaux CRUD, duplication, export/import de la démo ; migration additive des anciennes données.
- 🟡 Interface encore en démonstration : aucune fonction matérielle pilotée, macros et capacités spécifiques non raccordées.
- 🔴 Intégration production au moteur conservé et preuves matérielles toujours à réaliser séparément.

### Correction : barre conservée et présence réelle

- 🟢 Barre restaurée ; raccourci six points supprimé ; avatar texte/image personnalisable.
- 🟢 Énumération réelle VID 1532 avec regroupement des interfaces Windows, Dashboard et tray synchronisés au scan.
- 🟢 Cinq appareils/émetteurs USB réellement recensés sur le PC ; pas de batterie ou de carte fictive.
- 🟢 Détection passive partagée EXE / web local, état vide/erreur explicite.
- 🟡 Configuration matérielle toujours non raccordée : contrôles désactivés, ni batterie ni réglages courants lus, ni liaison radio prouvée.

### Langue et ressources produit

- 🟢 Anglais par défaut, module de traduction hors ligne et exemple de dictionnaire séparé.
- 🟢 Images produit configurées par les développeurs (modèle/PID), sans import utilisateur ; avatar personnalisable conservé.
- 🟡 Traductions complètes et sélecteur de langue à ajouter lorsque les dictionnaires seront fournis ; tray actuellement en anglais.
