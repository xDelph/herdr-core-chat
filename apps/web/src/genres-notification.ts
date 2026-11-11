/**
 * Quelles notifications cet appareil veut recevoir (Réglages, maquette
 * « Inbox ») : à toi, erreur, fini.
 *
 * ⚠️ **Le tri se fait sur le Mac, pas dans le service worker.** Sur iPhone,
 * un push reçu doit s'afficher (`userVisibleOnly`) : en avaler un en silence
 * peut faire révoquer l'abonnement. Le démon ne l'envoie donc pas — les genres
 * coupés voyagent avec l'abonnement (`muted`, upsert par endpoint).
 *
 * Le choix est aussi retenu ici : le démon ne le relit pas pour nous, et un
 * réabonnement (« Turn on ») doit le renvoyer.
 */
import type { SubscribeRequest } from "@herdr-core-chat/shared";

export type GenreNotification = "waiting" | "done" | "error" | "gone";

const GENRES: ReadonlyArray<GenreNotification> = ["waiting", "done", "error", "gone"];

/** Les interrupteurs des réglages : « erreur » couvre aussi l'agent qui n'est plus là. */
export const CHOIX_NOTIFICATIONS: ReadonlyArray<{
  readonly cle: string;
  readonly libelle: string;
  readonly genres: ReadonlyArray<GenreNotification>;
}> = [
  { cle: "attente", genres: ["waiting"], libelle: "An agent needs a decision" },
  { cle: "erreur", genres: ["error", "gone"], libelle: "An agent stops with an error" },
  { cle: "fini", genres: ["done"], libelle: "An agent finishes" },
];

export const CLE_GENRES_COUPES = "herdr-core-chat.notifications.coupees";

/** Relit le choix retenu ; toute valeur inconnue est ignorée (tout est reçu). */
export const genresCoupesEnregistres = (brut: string | null): GenreNotification[] => {
  if (brut === null) return [];
  try {
    const lu: unknown = JSON.parse(brut);
    if (!Array.isArray(lu)) return [];
    return GENRES.filter((genre) => lu.includes(genre));
  } catch {
    return [];
  }
};

/** Coupe (ou rétablit) les genres d'un interrupteur ; l'ordre reste celui de `GENRES`. */
export const basculeGenres = (
  coupes: ReadonlyArray<GenreNotification>,
  genres: ReadonlyArray<GenreNotification>,
  recevoir: boolean,
): GenreNotification[] =>
  GENRES.filter((genre) => (genres.includes(genre) ? !recevoir : coupes.includes(genre)));

/** Le corps d'abonnement : l'abonnement du navigateur, et ce qu'il ne veut pas recevoir. */
export const corpsAbonnement = (
  abonnement: PushSubscriptionJSON,
  coupes: ReadonlyArray<GenreNotification>,
): PushSubscriptionJSON & Pick<SubscribeRequest, "muted"> =>
  coupes.length === 0 ? abonnement : { ...abonnement, muted: [...coupes] };
