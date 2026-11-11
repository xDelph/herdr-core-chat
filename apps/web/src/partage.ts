/**
 * Le **partage de structure** : une relecture identique rend les MÊMES objets.
 *
 * ⚠️ **Chaque rafraîchissement redessinait toute l'application** (audit,
 * performance). Le JSON relu toutes les quelques secondes donne des objets
 * neufs, même quand rien n'a changé : aucune ligne ne gardait son identité,
 * et un `memo` ne servait à rien. Ici la valeur relue est comparée à la
 * précédente, et tout ce qui est égal est REPRIS de la précédente :
 *
 * - rien n'a changé → la précédente elle-même (aucun abonné n'est réveillé) ;
 * - une ligne a changé → un nouveau tableau, mais les autres lignes gardent
 *   leur identité (leur `memo` tient).
 *
 * Les tableaux d'objets portant un `paneId` sont appariés **par pane**, pas
 * par position : un agent qui remonte dans la liste reste le même objet.
 *
 * Valeurs JSON seulement (objets simples, tableaux, primitives) — c'est ce
 * que le démon sert.
 */

const estObjetSimple = (valeur: unknown): valeur is Record<string, unknown> =>
  typeof valeur === "object" && valeur !== null && !Array.isArray(valeur);

const paneDe = (valeur: unknown): string | undefined => {
  if (!estObjetSimple(valeur)) return undefined;
  const pane = valeur["paneId"];
  return typeof pane === "string" ? pane : undefined;
};

const partageTableau = (
  ancien: ReadonlyArray<unknown>,
  nouveau: ReadonlyArray<unknown>,
): ReadonlyArray<unknown> => {
  const parPane = new Map<string, unknown>();
  for (const element of ancien) {
    const pane = paneDe(element);
    if (pane !== undefined) parPane.set(pane, element);
  }
  let identique = ancien.length === nouveau.length;
  const resultat = nouveau.map((element, index) => {
    const pane = paneDe(element);
    const vis = pane === undefined ? ancien[index] : parPane.get(pane);
    const partage = vis === undefined ? element : partageStructure(vis, element);
    if (partage !== ancien[index]) identique = false;
    return partage;
  });
  return identique ? ancien : resultat;
};

const partageObjet = (
  ancien: Record<string, unknown>,
  nouveau: Record<string, unknown>,
): Record<string, unknown> => {
  const cles = Object.keys(nouveau);
  let identique = cles.length === Object.keys(ancien).length;
  const resultat: Record<string, unknown> = {};
  for (const cle of cles) {
    const partage = Object.hasOwn(ancien, cle)
      ? partageStructure(ancien[cle], nouveau[cle])
      : nouveau[cle];
    if (!Object.hasOwn(ancien, cle) || partage !== ancien[cle]) identique = false;
    resultat[cle] = partage;
  }
  return identique ? ancien : resultat;
};

/**
 * `nouveau`, avec tout ce qui est égal à `ancien` repris d'`ancien` — et
 * `ancien` lui-même si tout est égal.
 */
export const partageStructure = <T>(ancien: T, nouveau: T): T => {
  if (Object.is(ancien, nouveau)) return ancien;
  if (Array.isArray(ancien) && Array.isArray(nouveau)) {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- même forme JSON, tableau pour tableau
    return partageTableau(ancien, nouveau) as T;
  }
  if (estObjetSimple(ancien) && estObjetSimple(nouveau)) {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- même forme JSON, objet pour objet
    return partageObjet(ancien, nouveau) as T;
  }
  return nouveau;
};
