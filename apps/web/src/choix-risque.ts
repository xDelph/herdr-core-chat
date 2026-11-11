/**
 * Les choix d'invite dont l'effet DURE au-delà de la question : « Yes, and
 * don't ask again », « Always allow », « Allow for this session »…
 *
 * ⚠️ **Un seul appui ne suffit pas pour ceux-là** (F1.5). En faisant défiler la
 * boîte de réception, le pouce tombait sur « Yes, and don't ask again » : une
 * permission accordée pour toutes les commandes suivantes, sans l'avoir lue.
 * La liste est volontairement large — un second appui de trop coûte moins
 * qu'une permission permanente donnée par erreur.
 */
const MOTIFS: ReadonlyArray<RegExp> = [
  /don['’]t ask again/i,
  /\balways\b/i,
  /\bfor this session\b/i,
  /\ball (?:edits|commands|tools)\b/i,
  /\bbypass\b/i,
  /\bauto[- ]?accept\b/i,
  /\bremember\b/i,
];

export const estChoixARisque = (libelle: string, detail?: string): boolean =>
  MOTIFS.some((motif) => motif.test(libelle) || (detail !== undefined && motif.test(detail)));

/** Le délai au-delà duquel un choix armé se désarme de lui-même. */
export const DELAI_CONFIRMATION_MS = 4000;
