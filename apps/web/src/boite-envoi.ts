/**
 * La boîte d'envoi (lot 4, maquette D) : les envois **non résolus** de TOUS
 * les agents, au même endroit.
 *
 * ⚠️ **Pourquoi elle existe.** Un envoi « non confirmé » ne se voyait que dans
 * la bulle de SA conversation : en changeant d'agent, il disparaissait de
 * l'écran sans être tranché — peut-être arrivé, peut-être pas (F1.9, F1.13,
 * F3.3). Ici il reste visible tant que l'utilisateur ne l'a pas fermé.
 *
 * ⚠️ **Rien n'est renvoyé tout seul.** La boîte montre et propose ; les
 * gestes sont ceux de la bulle (`chat/Envois.tsx`), avec les mêmes libellés
 * (« may arrive twice » quand le message a peut-être été remis).
 */
import type { Envoi } from "./envois.ts";

/**
 * Le genre d'un envoi dans la boîte — ce qu'il faut en penser d'un regard.
 *
 * - `en-cours` : en file ou en cours d'envoi, rien à faire ;
 * - `non-confirme` : peut-être arrivé — vérifier avant de renvoyer ;
 * - `non-envoye` : rien n'est parti (refus, hors ligne, doublon écarté).
 */
export type GenreBoite = "en-cours" | "non-confirme" | "non-envoye";

/** Un envoi de la boîte, avec son genre. */
export interface EntreeBoite {
  readonly envoi: Envoi;
  readonly genre: GenreBoite;
}

/** Le genre d'un envoi, ou `null` s'il est résolu (remis : hors de la boîte). */
export const genreDe = (envoi: Envoi): GenreBoite | null => {
  switch (envoi.state) {
    case "queued":
    case "sending":
      return "en-cours";
    case "uncertain":
      return "non-confirme";
    case "failed":
    case "suppressed":
      return "non-envoye";
    case "delivered":
      return null;
  }
  return null;
};

/** Le libellé court du genre, tel que la boîte l'affiche. */
export const LIBELLE_GENRE: Record<GenreBoite, string> = {
  "en-cours": "Sending",
  "non-confirme": "Not confirmed",
  "non-envoye": "Not sent",
};

/** L'ordre d'affichage : ce qui demande un regard d'abord. */
const RANG: Record<GenreBoite, number> = {
  "non-confirme": 0,
  "non-envoye": 1,
  "en-cours": 2,
};

/**
 * Les envois de la boîte : non résolus seulement, ceux qui demandent un
 * regard d'abord, puis du plus récent au plus ancien.
 */
export const entreesBoite = (envois: ReadonlyArray<Envoi>): ReadonlyArray<EntreeBoite> =>
  envois
    .flatMap((envoi) => {
      const genre = genreDe(envoi);
      return genre === null ? [] : [{ envoi, genre }];
    })
    .toSorted((a, b) => RANG[a.genre] - RANG[b.genre] || b.envoi.creeA - a.envoi.creeA);

/** Le compte de la pastille : combien en tout, combien demandent un regard. */
export const resumeBoite = (
  entrees: ReadonlyArray<EntreeBoite>,
): { readonly total: number; readonly aRegarder: number } => ({
  aRegarder: entrees.filter((entree) => entree.genre !== "en-cours").length,
  total: entrees.length,
});

/** Le titre de la boîte : « 2 need a look », sinon « Sending… ». */
export const sousTitreBoite = (resume: {
  readonly total: number;
  readonly aRegarder: number;
}): string => {
  if (resume.total === 0) return "Nothing waiting";
  if (resume.aRegarder === 0) return `${String(resume.total)} sending`;
  return resume.aRegarder === 1 ? "1 needs a look" : `${String(resume.aRegarder)} need a look`;
};
