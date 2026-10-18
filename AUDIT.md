# Audit Secbird — 18 octobre 2026

Base locale : `9f6d0cac305c093ae2f582614e8fed1d48205481`, workspace 0.4.5. Audit statique, sans découverte USB ni lancement du tray. Les résultats ci-dessous ne constituent pas une certification matérielle. Inventaire de tout le dépôt effectué ; revue approfondie des frontières UI/API/transport/profils et revue des familles de fonctions. Pas de preuve dynamique de chaque chemin ni d'audit de sécurité exhaustif.

Complément de livraison : `prototype/` est un workspace indépendant de démonstration, avec catalogue déclaratif partagé par le frontend et le tray Tauri. Composants réutilisables et pages par famille, profils centralisés et visuels remplaçables par PNG. Aucun fichier du backend Secbird historique modifié. Cette migration UI ne constitue pas un raccordement ni une validation de ses fonctions matérielles. Architecture de cette base : `prototype/COMPONENTS.md`.

Dernière correction : `prototype/` référence maintenant la bibliothèque core existante uniquement pour sa DB et l'énumération HID passive. Dashboard et tray utilisent les appareils effectivement présents, non la liste statique des exemples. `src/discovery.rs` ne passe pas par `core::discover` et ne transmet aucun rapport de protocole ; l'ouverture temporaire de handle sert à lire le Container ID PnP via l'API Windows. Cinq appareils/émetteurs USB effectivement recensés sur le PC pendant cette mission (00D7, 0258, 0C01, 0F1F, 0F21). La batterie, les réglages courants et la présence radio ne sont pas vérifiés par ce scan. Les fichiers core historiques restent inchangés.

## Architecture et compilation

Rust 2021, workspace Cargo à trois crates. `secbird-core` contient le protocole, la base de périphériques et les profils ; `secbird` est le CLI ; `secbird-tray` fournit le cycle de vie Win32, le serveur local et l'éditeur. Aucune dépendance React, Tauri, Electron ou WebView2 actuellement.

| Ensemble | Responsabilité et observations |
|---|---|
| core/lib, error, log | exports, erreurs, journal local |
| core/proto, commands | rapports de 90 octets, checksum XOR, commandes Protocol 2.5 |
| core/transport, device | trait Transport, hidapi, découverte, identification des récepteurs, requêtes/réponses et retries |
| core/db, devices.json | capacités déclaratives par PID ; surcharge utilisateur ; déclaration ≠ validation |
| core/features, effects | DPI X/Y, stages, polling, batterie, veille, dimming, couleurs, matrices, effets matériels |
| core/buttons, macros | encodage des affectations et macros embarquées ; chemins pouvant écrire la mémoire du périphérique |
| core/actions, profile | actions partagées CLI/UI, sauvegarde JSON, profils par identité physique, réapplication |
| core/sim | Transport simulé et tests ; simule des réponses, ne prouve pas le support matériel |
| core/report | rapport de diagnostic ; lectures de protocole, donc émission de requêtes HID |
| core/net, update, firmware | WinHTTP, vérification GitHub, installation signée de l'application, consultation de manifests Razer ; aucun flash firmware implémenté |
| core/debloat | inventaire et modification de services/démarrage Razer avec sauvegarde ; à exclure du prototype |
| tray/main, win | mutex d'instance, boucle Win32, notifications, reprise/replug, tray, lancement de l'éditeur par ShellExecute |
| tray/apps, osd, settings | application au premier plan, popup DPI/profil, préférences persistantes |
| tray/engine | HidApi, appareils, Store, application des profils, batteries et événements ; indépendant de Win32 mais actuellement dans la crate tray |
| tray/server | serveur TCP std, JSON, moteur derrière Arc<Mutex<Engine>>, sérialisation et appels directs actions/Device/Store |
| tray/editor.html | HTML/CSS/JS embarqué, interface monolithique, thèmes, fetch API, profils et panneaux ; navigateur externe |
| CLI/main | commandes de configuration, diagnostic, profils, macros, debloat et mises à jour |
| assets, docs, site | marques Secretarybird, captures, site vitrine statique ; aucun de ces fichiers remplacé |
| tools | restatement OpenRazer, sonde HID Python et extraction de caches/commandes ; aucun script exécuté pendant cette mission |
| installer, packaging | Inno Setup Windows par utilisateur, CLI Linux et règles udev |
| .github | CI Windows/Linux clippy et tests ; release Windows/MSVC et Linux/musl ; Pages pour le site vitrine |

Dépendances principales : serde/serde_json, hidapi 2.6 (lock 2.6.7 ; backend windows-native), windows-sys 0.59 et embed-resource 2.5. Les dépendances transitives sont verrouillées dans Cargo.lock. Compilation existante : `cargo build --release`, tests `cargo test --release`, clippy comme en CI. Release : opt-level z, LTO, un codegen-unit, strip, panic abort. Inno Setup 6 produit l'installateur. Pas de toolchain Rust détectée dans le PATH de cette session : compilation du backend non vérifiée.

## Couplage et sécurité du mode démonstration

Chemin réel : éditeur → serveur localhost → actions/Engine/Store → Device → Transport → hidapi. La crate core est déjà réutilisable. L'API mélange présentation, lecture matériel, stockage et application : par exemple device_json lit DPI, batterie, macros et firmware ; le mutex moteur peut rester occupé pendant les échanges USB.

Le serveur écoute `127.0.0.1:0`, vérifie Host/Origin et un jeton de session, refuse le CORS et envoie CSP et en-têtes de protection. Ces mécanismes sont à conserver tant que l'ancien éditeur existe. Le protocole HTTP artisanal et les appels bloquants nécessitent une revue dédiée avant d'exposer une nouvelle API.

**SECBIRD_SIM n'est pas un mode sans matériel** : discover appelle discover_hid puis ajoute les simulations. `--serve` appelle engine.rescan ; le tray réapplique les profils aux événements. Même un GET logique envoie un feature report. Ne lancer ni le tray, ni `--serve`, ni probe.py pour la démo. Le nouveau projet n'importe pas secbird-core et ne contient aucune route vers HID.

## Fonctionnalités : existence et niveau de preuve

| Fonction | Implémentation | Limite de preuve |
|---|---|---|
| DPI/stages/polling | features + actions + UI/CLI | tests simulés et validations rapportées sur certains modèles ; pas sur les appareils de l'utilisateur |
| RGB/effets/per-key | features/effects + matrices DB | support variable par protocole et produit ; pas tous les effets partout |
| Remapping/HyperShift | buttons + actions, undo/restauration et protection du dernier clic gauche | souris avec mémoire compatible ; ne pas généraliser au remapping clavier |
| Macros | encodage, stockage et affectation embarqués | peut écrire la flash ; interdit dans cette phase |
| Batterie/veille | lectures et réglages sans fil | une erreur est parfois réduite à null/absence dans l'API |
| Profils locaux | Store JSON, import/export, changement par application | identité PID:serial ; sans serial repli PID ; USB et dongle peuvent avoir des identités distinctes |
| Mémoire embarquée | lecture des slots et utilisation du slot live pour boutons, macros | pas un gestionnaire universel des slots Synapse ; les entrées ciblées n'annoncent pas onboard_profiles |
| Tray/auto-start/instance unique | code Win32 et registre, mutex Local\\Secretarybird.Tray | déjà utile, mais aucune fenêtre WebView2 actuelle |
| Updates et firmware | code réel réseau, installation de l'application après action utilisateur | quotidien GitHub par défaut ; firmware Razer sur demande ou hebdomadaire opt-in ; incompatible avec un mode réseau strictement interdit sans adaptation |
| Casques/Bluetooth | affichage ou déclaration possible | pas de contrôle casque Kraken ni support Bluetooth annoncé |
| Smart Dimming Blade | code et simulation | README : non testé sur laptop réel |

Le README distingue matériel vérifié, retours utilisateurs et appareils simplement listés ; les captures de l'éditeur utilisent aussi des simulations. Les tests valident des encodages/comportements contre simulateur, pas la conformité firmware.

## Matériels demandés

Précision utilisateur : modèle possédé **Basilisk V3 Pro 35K Phantom Green Edition** ; les PID prioritaires sont donc 00D6/00D7. BlackWidow V3 Mini HyperSpeed également possédé. Inventaire complémentaire déclaré : Goliathus Chroma, Thunderbolt 4 Dock Chroma et ARGB Controller. Il s'agit de l'inventaire utilisateur, sans énumération USB réalisée par l'agent.

VID commun 1532. Données lues dans devices.json, pas mesurées sur USB.

| Produit / connexion | PID | Déclaration locale | Conclusion |
|---|---|---|---|
| Basilisk V3 Pro 35K USB | 00CC | txid 1F, 100–35000 DPI, pas 50, polling standard, logo/molette | listé, non validé ici |
| Basilisk V3 Pro 35K 2,4 GHz | 00CD | mêmes déclarations | listé, non validé ici ; pas de preuve de pairage multi-appareils |
| Phantom Green USB / 2,4 GHz | 00D6 / 00D7 | molette seule déclarée | README : retour utilisateur sur cette variante ; pas une preuve pour 00CC/00CD |
| BlackWidow V3 Mini USB | 0258 | txid 1F, RGB extended, matrice 5×16, wireless true | listé, non validé ici |
| BlackWidow V3 Mini 2,4 GHz | 0271 | txid 9F, même matrice | README : retour utilisateur global, connexion non précisée ; non validé par transport |

Pas de preuve autorisant 8000 Hz sur 00CC/00CD : la DB annonce polling standard (125/500/1000). HyperPolling et Mouse Dock Pro ont des chemins séparés dans device.rs ; ne pas assimiler ces récepteurs à HyperSpeed standard. Un dongle partagé clavier/souris, les reconnexions, la batterie et les différences de firmware restent à tester séparément. Bluetooth hors périmètre supporté.

| Appareil complémentaire | Entrée déclarée | Limite |
|---|---|---|
| Goliathus Chroma | 0C01 Goliathus 2018 / 0C02 Extended / 0C06 3XL, txid 3F, une zone et matrice 1×1 | variante exacte non précisée ; README : retour utilisateur global, pas une preuve par variante |
| Thunderbolt 4 Dock Chroma | 0F21, txid 1F, zone all, matrice 1×12 | README : retour utilisateur, pas validation indépendante |
| ARGB Controller | 0F1F Chroma Addressable RGB Controller, txid 3F, matrice 6×80 | README : détecté mais pas d'écran de configuration ; ne pas déduire la gestion des ports/longueurs LED de la seule matrice |

Captures de l'utilisateur attendues dans `references/synapse4/`, ignorées par Git et exclues des assets de l'application. Les trois accessoires sont inventoriés ; ils ne sont pas encore représentés dans la première démo à deux périphériques.

## Licence, crédits et provenance

Licence du projet : **MIT OR Apache-2.0**, copyright 2026 infinityInfinite. Garder LICENSE-MIT, LICENSE-APACHE, NOTICE et les crédits dans les distributions. Sous MIT, conserver copyright et texte de permission ; sous Apache, fournir licence et mentions requises, identifier les fichiers modifiés et préserver les notices applicables. Inventorier aussi les licences des dépendances et futurs assets dans les binaires distribués. L'analyse technique ne tranche pas les droits de marque ou l'habillage commercial.

NOTICE décrit des faits matériels reformulés depuis OpenRazer GPL-2.0, commit 477f4d6 ; l'affirmation « aucun code copié » vient de l'amont et n'est pas une analyse historique indépendante. Point de provenance à examiner avant redistribution : tools/dump_cache.py et extract_cmds.py traitent des caches/JS ; plusieurs commentaires (buttons, effects, macros) et sources DB citent Synapse. Leur présence ne démontre pas une copie illicite, mais interdit d'affirmer une provenance clean-room intégralement établie. Ne pas les utiliser pour la reproduction visuelle. Aucun code, cache, logo, rendu produit ou police propriétaire extrait pour cette mission.

Référence amont : https://github.com/infinityInfinite/secbird ; le commit local ci-dessus fait foi si l'amont évolue. Résultats de vérification du prototype et limites : DEVELOPMENT_LOG.md.
