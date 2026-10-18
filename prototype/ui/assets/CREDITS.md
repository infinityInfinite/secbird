# Ressources visuelles actives

Dernière décision utilisateur : icônes simples par défaut ; aucune photo stock ni illustration produit. Les deux fichiers JPEG Pexels ont été retirés du frontend. Les crédits historiques ci-dessous documentent uniquement la précédente itération, ils ne décrivent plus des ressources embarquées.

`secbird.png` et l'icône EXE/tray viennent du projet Secretarybird d'infinityInfinite, sans modification. Les pictogrammes standard de navigation et de catégorie sont définis dans `ui/visuals.js`. Les futurs PNG se configurent dans `ui/device-assets.js`  ; leur provenance devra être ajoutée ici avant redistribution.

## Historique des photos retirées

Photos stock intégrées localement, téléchargées le 18 octobre 2026. Ce sont des modèles génériques, pas des photographies des appareils nommés dans la démo. Remplacer ces fichiers à l'arrivée des ressources définitives. Aucune requête distante à l'exécution.

- `stock-mouse.jpg` : Sogi ., « White Computer Mouse », Pexels, photo 17821147. https://www.pexels.com/photo/white-computer-mouse-17821147/
- `stock-keyboard.jpg` : Brayden Gale, « Wireless Keyboard on Black Background », Pexels, photo 21705974. https://www.pexels.com/photo/wireless-keyboard-on-black-background-21705974/
- Licence des photos : Pexels, https://www.pexels.com/license/ ; les fichiers ne sont pas sous la licence MIT/Apache du code. Versions optimisées à 600 px fournies par le CDN Pexels ; aucun détourage ni dessin de produit ajouté.
- `secbird.png` : copie non modifiée de `assets/logo-64.png`, projet Secretarybird d'infinityInfinite. Mentions amont et licences conservées.

Palette Bridge reprise de `crates/secbird-tray/src/editor.html` : fond #050505, barre #0d0d0e, panneau #131314, accent #ff5412, valeurs #ffb85d, texte #f4efe9. La structure des vues s'inspire des captures utilisateur ; l'identité visuelle est celle de Secbird.
