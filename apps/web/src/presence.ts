/**
 * La présence de CET appareil (H23) : l'app au premier plan le dit au démon,
 * et tant qu'elle y est, **aucune notification ne part** — ni ici, ni sur les
 * autres appareils.
 *
 * ⚠️ **Retours du 2026-10-29 et du 2026-11-12.** « Je suis dans l'app, et je
 * reçois quand même la notif » ; puis : PWA ouverte sur la tablette, le
 * téléphone notifié quand même. Le worker ne peut pas se taire (iOS révoque un
 * abonnement qui reçoit des push sans notification) : le tri se fait côté
 * démon, avant l'envoi, sur la présence de n'importe quel appareil.
 *
 * - visible : un battement tout de suite, puis toutes les 10 s (le démon
 *   oublie une présence au bout de 25 s : un appareil gelé sans avoir dit
 *   qu'il partait redevient notifiable de lui-même) ;
 * - caché : le départ part tout de suite (`keepalive`, la page peut geler
 *   juste après).
 *
 * ⚠️ **Un appareil SANS notifications activées signale sa présence quand même**
 * (`endpoint: null`) : la PWA ouverte sur la tablette doit retenir le
 * téléphone. Son identifiant (`appareil`) le nomme.
 */

export const BATTEMENT_PRESENCE_MS = 10_000;

/** Ce que la présence dit au démon : qui (appareil, endpoint s'il existe), et l'état. */
export interface Signalement {
  readonly appareil: string;
  readonly endpoint: string | null;
  readonly visible: boolean;
}

export interface DependancesPresence {
  /** L'identifiant stable de cette PWA — présent même sans abonnement push. */
  readonly appareil: () => string;
  /** L'endpoint push de cet appareil, ou `null` s'il n'est pas abonné. */
  readonly endpoint: () => Promise<string | null>;
  readonly envoie: (signalement: Signalement) => Promise<void>;
  readonly visible: () => boolean;
  /** Répète une action ; rend l'arrêt. */
  readonly repete: (action: () => void, intervalle: number) => () => void;
  /** Une panne d'envoi, avec sa cause — jamais avalée. */
  readonly signale: (cause: string) => void;
}

export interface Presence {
  /** À appeler à chaque changement de visibilité (et au démarrage). */
  readonly surVisibilite: () => Promise<void>;
  readonly arrete: () => void;
}

export const creePresence = (deps: DependancesPresence): Presence => {
  let arreteBattement: (() => void) | null = null;
  /** Le dernier endpoint connu : le départ n'attend pas de le relire (gel de la page). */
  let dernierEndpoint: string | null = null;

  const envoie = async (signalement: Signalement): Promise<void> => {
    try {
      await deps.envoie(signalement);
    } catch (cause) {
      deps.signale(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const bat = async (): Promise<void> => {
    // ⚠️ Lire l'abonnement peut échouer (service worker absent, permission
    // retirée) : la cause est dite, et la présence part quand même, sans endpoint.
    let endpoint: string | null;
    try {
      endpoint = await deps.endpoint();
    } catch (cause) {
      deps.signale(cause instanceof Error ? cause.message : String(cause));
      endpoint = dernierEndpoint;
    }
    dernierEndpoint = endpoint;
    await envoie({ appareil: deps.appareil(), endpoint, visible: true });
  };

  return {
    arrete: () => {
      arreteBattement?.();
      arreteBattement = null;
    },
    surVisibilite: async () => {
      if (deps.visible()) {
        if (arreteBattement === null) {
          arreteBattement = deps.repete(() => void bat(), BATTEMENT_PRESENCE_MS);
        }
        await bat();
        return;
      }
      arreteBattement?.();
      arreteBattement = null;
      // Le départ part toujours, même si l'appareil n'a jamais battu.
      await envoie({ appareil: deps.appareil(), endpoint: dernierEndpoint, visible: false });
    },
  };
};

const CLE_APPAREIL = "herdr-core-chat.appareil";

/**
 * L'identifiant stable de cette PWA : créé une fois, gardé dans le stockage du
 * navigateur. Sans stockage (navigation privée, stockage refusé), un
 * identifiant de session : la présence reste nommée pendant l'ouverture.
 */
let identifiant: string | null = null;

const neuf = (): string =>
  typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `appareil-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;

export const identifiantAppareil = (): string => {
  if (identifiant !== null) return identifiant;
  try {
    const existant = window.localStorage.getItem(CLE_APPAREIL);
    if (existant !== null && existant.length > 0) {
      identifiant = existant;
      return existant;
    }
    identifiant = neuf();
    window.localStorage.setItem(CLE_APPAREIL, identifiant);
  } catch {
    // Pas de stockage : l'identifiant de cette page suffit (il n'est pas relu).
    identifiant = neuf();
  }
  return identifiant;
};
