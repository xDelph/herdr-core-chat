/**
 * La **fenêtre glissante** du fil : seuls les derniers tours sont dans le DOM.
 *
 * ⚠️ **Le fil n'était ni virtualisé ni élagué** (audit, performance). Chaque
 * page d'historique remontée restait rendue, et chaque arrêt du défilement
 * mesurait TOUS les éléments. Une longue session rendait des milliers de
 * blocs Markdown pour en montrer une dizaine.
 *
 * La fenêtre se compte en **tours** (un tour s'ouvre sur une demande), pas en
 * pixels : une virtualisation par hauteur estimée remplaçait des hauteurs
 * devinées par les vraies, et Safari (sans « scroll anchoring ») faisait
 * sauter la lecture — c'est le défaut que `content-visibility` avait déjà
 * produit (`Fil.tsx`). Ici rien n'est estimé : la fenêtre grandit vers le
 * haut par paquets, et la position est corrigée sur les hauteurs MESURÉES,
 * comme pour une page d'historique (`positionApresPrepend`).
 *
 * Bornes :
 * - au moins `TOURS_FENETRE` tours et `ELEMENTS_MIN` éléments ;
 * - au plus `ELEMENTS_MAX` éléments — sauf pour garder la dernière demande :
 *   la réponse se lit avec sa question (`versDerniereDemande`).
 *
 * Le module est pur : la fenêtre est un index de début, le composant le tient.
 */

/** Les tours rendus d'emblée, et à chaque extension vers le haut. */
export const TOURS_FENETRE = 10;
/** Le minimum d'éléments rendus (des tours très courts). */
export const ELEMENTS_MIN = 60;
/** Le maximum d'éléments rendus (des tours très longs) — hors dernière demande. */
export const ELEMENTS_MAX = 200;
/**
 * La marge avant de RE-glisser une fenêtre qui a grandi par le bas : sans
 * elle, chaque message ajouté retirerait un élément en haut.
 */
export const MARGE_GLISSEMENT = 60;

/**
 * Le début d'une fenêtre « par défaut » qui finit sur le dernier élément.
 *
 * @param estDemande dit si l'élément `index` ouvre un tour.
 * @param fin l'index (exclu) où la fenêtre se termine — `longueur` pour le bas.
 */
export const debutParDefaut = (
  longueur: number,
  estDemande: (index: number) => boolean,
  fin: number = longueur,
  tours: number = TOURS_FENETRE,
): number => {
  if (fin <= 0) return 0;
  let tour = 0;
  let debutParTours = 0;
  let derniereDemande: number | null = null;
  for (let index = fin - 1; index >= 0; index -= 1) {
    if (!estDemande(index)) continue;
    derniereDemande ??= index;
    tour += 1;
    if (tour >= tours) {
      debutParTours = index;
      break;
    }
  }
  // Au moins `ELEMENTS_MIN` éléments, au plus `ELEMENTS_MAX`…
  const borne = Math.max(fin - ELEMENTS_MAX, Math.min(fin - ELEMENTS_MIN, debutParTours));
  // … sans jamais couper la dernière demande de sa réponse.
  const debut = derniereDemande === null ? borne : Math.min(borne, derniereDemande);
  return Math.max(0, debut);
};

/**
 * Le début après une extension vers le haut : `TOURS_FENETRE` tours de plus
 * au-dessus du début actuel (bornés comme la fenêtre par défaut).
 */
export const debutEtendu = (debut: number, estDemande: (index: number) => boolean): number =>
  debut <= 0 ? 0 : Math.min(debut - 1, debutParDefaut(debut, estDemande, debut));

/**
 * La fenêtre a-t-elle trop grandi par le bas ? Seulement quand on SUIT le bas :
 * retirer du contenu au-dessus de quelqu'un qui lit le ferait sauter.
 */
export const doitReglisser = (debut: number, longueur: number, suitLeBas: boolean): boolean =>
  suitLeBas && longueur - debut > ELEMENTS_MAX + MARGE_GLISSEMENT;

/**
 * Le début qui garde l'élément `ancre` dans la fenêtre — la reprise de lecture
 * d'un message plus ancien que la fenêtre par défaut.
 */
export const debutPourAncre = (debut: number, ancre: number): number =>
  ancre >= 0 && ancre < debut ? ancre : debut;
