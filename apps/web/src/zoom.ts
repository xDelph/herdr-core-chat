/**
 * Le retour sur la PWA depuis le sélecteur d'apps / le gestionnaire de fenêtres
 * de l'iPad (deux doigts vers le haut) : la vue restait agrandie (retour du
 * 2026-11-13 : « je dois dessiner à la main pour la remettre »).
 *
 * Safari ne propose pas de remise à zéro du zoom en JavaScript. Le contournement
 * connu : réaffecter la meta `viewport` force iOS/iPadOS à recalculer la mise en
 * page à l'échelle initiale. Sans effet quand rien n'est zoomé.
 *
 * ⚠️ Non vérifié sur l'iPad réel : le geste de fenêtrage ne se reproduit pas
 * dans le navigateur de test. La cause côté champs (zoom au focus) est, elle,
 * traitée par la police de 16 px des champs (`styles/base.css`).
 */

/** La meta du document, telle qu'écrite dans `index.html`. */
export const CONTENU_VIEWPORT =
  "width=device-width, initial-scale=1, viewport-fit=cover, interactive-widget=resizes-content";

/**
 * Réaffecte la meta `viewport` pour remettre l'échelle à zéro. Rend `true` si la
 * meta existe et a été réaffectée.
 */
export const reinitialiseZoom = (meta: { content: string } | null): boolean => {
  if (meta === null) return false;
  // Une valeur neutre puis la vraie : iOS recalcule à chaque changement.
  meta.content = "width=device-width, initial-scale=1";
  meta.content = CONTENU_VIEWPORT;
  return true;
};
