/**
 * Le centre de connexion (lot 4, maquette B) : la chaîne entière, maillon par
 * maillon — cet appareil, le démon, herdr-core, le direct, la veille.
 *
 * ⚠️ **Il remplace le « Offline » fourre-tout (F3.7).** L'indicateur disait un
 * mot pour cinq causes différentes ; ici chaque maillon dit son état, avec ce
 * que le démon en sait (`/api/health` : `herdr.error`, `watcher`). Un maillon
 * dont on ne sait rien le dit (« unknown »), il n'est jamais réputé sain.
 *
 * ⚠️ **Aucun contenu de message** : seulement des états, des versions, des
 * âges et des causes d'erreur.
 */
import type { Health } from "./api.ts";
import type { EtatDonnees } from "./donnees.ts";
import { dureeLongue } from "./libelles.ts";

/** Le ton d'un maillon — la couleur porte l'état (`tokens.css`). */
export type TonMaillon = "ok" | "attention" | "erreur" | "inconnu";

/** Un maillon de la chaîne. */
export interface Maillon {
  readonly cle: "appareil" | "demon" | "core" | "direct" | "veille";
  readonly titre: string;
  readonly ton: TonMaillon;
  readonly texte: string;
  /** Une cause ou un détail, tel que le démon l'a dit. */
  readonly detail: string | null;
}

/**
 * Au-delà de deux battements (15 s) sans signe de vie, le direct est en
 * retard : le flux sera bientôt recréé (`SILENCE_MAX_FLUX_MS`).
 */
export const RETARD_DIRECT_MS = 2 * 15_000;

/** Une veille dont le dernier succès date de plus de 30 s est en retard. */
export const RETARD_VEILLE_MS = 30_000;

const il = (ms: number): string => `${dureeLongue(ms)} ago`;

const maillonAppareil = (enLigne: boolean): Maillon => ({
  cle: "appareil",
  detail: null,
  texte: enLigne ? "Online" : "Offline: the device has no network",
  titre: "This device",
  ton: enLigne ? "ok" : "erreur",
});

const maillonDemon = (donnees: EtatDonnees, maintenant: number): Maillon => {
  const erreur = donnees.erreurs.health ?? donnees.erreurs.conversations;
  const vu =
    donnees.dernierSucces === null ? null : `last read ${il(maintenant - donnees.dernierSucces)}`;
  if (erreur !== null) {
    return {
      cle: "demon",
      detail: erreur,
      texte: vu === null ? "Not answering" : `Not answering · ${vu}`,
      titre: "Daemon",
      ton: "erreur",
    };
  }
  if (donnees.health === null && donnees.dernierSucces === null) {
    return { cle: "demon", detail: null, texte: "Not read yet", titre: "Daemon", ton: "inconnu" };
  }
  const uptime =
    donnees.health?.uptimeMs === undefined ? null : `up ${dureeLongue(donnees.health.uptimeMs)}`;
  return {
    cle: "demon",
    detail: null,
    texte: ["Answering", uptime, vu].filter((morceau) => morceau !== null).join(" · "),
    titre: "Daemon",
    ton: "ok",
  };
};

const maillonCore = (health: Health | null): Maillon => {
  if (health === null) {
    return { cle: "core", detail: null, texte: "Unknown", titre: "herdr-core", ton: "inconnu" };
  }
  const { herdr } = health;
  if (!herdr.reachable) {
    return {
      cle: "core",
      detail: herdr.error ?? null,
      texte: "Unreachable",
      titre: "herdr-core",
      ton: "erreur",
    };
  }
  const morceaux = [
    herdr.version ?? "version unknown",
    herdr.protocol === undefined ? null : `protocol ${String(herdr.protocol)}`,
  ].filter((morceau) => morceau !== null);
  return { cle: "core", detail: null, texte: morceaux.join(" · "), titre: "herdr-core", ton: "ok" };
};

const maillonDirect = (
  donnees: EtatDonnees,
  derniereVie: number | null,
  maintenant: number,
): Maillon => {
  const age = derniereVie === null ? null : maintenant - derniereVie;
  const vu = age === null ? null : `last sign of life ${il(age)}`;
  if (donnees.direct === "down") {
    return {
      cle: "direct",
      detail: null,
      texte: vu === null ? "Reconnecting…" : `Reconnecting… · ${vu}`,
      titre: "Live updates",
      ton: "attention",
    };
  }
  if (donnees.direct === "inconnu") {
    return {
      cle: "direct",
      detail: null,
      texte: "Connecting…",
      titre: "Live updates",
      ton: "inconnu",
    };
  }
  if (age !== null && age > RETARD_DIRECT_MS) {
    return {
      cle: "direct",
      detail: "No heartbeat from the daemon: the stream will be reopened.",
      texte: `Silent · ${vu ?? ""}`,
      titre: "Live updates",
      ton: "attention",
    };
  }
  return {
    cle: "direct",
    detail: null,
    texte: vu === null ? "Live" : `Live · ${vu}`,
    titre: "Live updates",
    ton: "ok",
  };
};

const maillonVeille = (health: Health | null): Maillon => {
  const veille = health?.watcher;
  if (veille === undefined) {
    return {
      cle: "veille",
      detail: null,
      texte: health === null ? "Unknown" : "Not reported by the daemon",
      titre: "Watcher",
      ton: "inconnu",
    };
  }
  const erreurs =
    veille.errors === 0
      ? null
      : veille.errors === 1
        ? "1 error"
        : `${String(veille.errors)} errors`;
  const succes =
    veille.lastSuccessAgoMs === undefined ? null : `last refresh ${il(veille.lastSuccessAgoMs)}`;
  const derniere =
    veille.lastError === undefined
      ? null
      : veille.lastErrorAgoMs === undefined
        ? veille.lastError
        : `${veille.lastError} (${il(veille.lastErrorAgoMs)})`;
  if (!veille.running) {
    return {
      cle: "veille",
      detail: derniere,
      texte: "Stopped: no live status, no push",
      titre: "Watcher",
      ton: "erreur",
    };
  }
  /*
    ⚠️ **Le flux d'événements de herdr-core (lot 4)** : branché, la veille
    repasse dès qu'un agent change (moins d'une seconde) ; perdu, elle relit
    toutes les 2 s — rien n'est raté, tout arrive plus tard. Un démon qui ne
    le dit pas (`events` absent) n'est pas réputé branché.
  */
  const flux =
    veille.events === true
      ? veille.lastEventAgoMs === undefined
        ? "core events live"
        : `core events live (last ${il(veille.lastEventAgoMs)})`
      : veille.events === false
        ? "polling every 2 s"
        : null;
  const enRetard =
    veille.lastSuccessAgoMs === undefined || veille.lastSuccessAgoMs > RETARD_VEILLE_MS;
  const ton: TonMaillon = veille.failing ? "erreur" : enRetard ? "attention" : "ok";
  return {
    cle: "veille",
    // La dernière panne reste utile après un rétablissement : on la montre.
    detail: derniere,
    texte: [veille.failing ? "Failing" : "Running", flux, succes, erreurs]
      .filter((morceau) => morceau !== null)
      .join(" · "),
    titre: "Watcher",
    ton,
  };
};

/** Les cinq maillons, dans l'ordre de la chaîne. */
export const maillons = (etat: {
  readonly donnees: EtatDonnees;
  readonly enLigne: boolean;
  readonly derniereVie: number | null;
  readonly maintenant: number;
}): ReadonlyArray<Maillon> => [
  maillonAppareil(etat.enLigne),
  maillonDemon(etat.donnees, etat.maintenant),
  maillonCore(etat.donnees.health),
  maillonDirect(etat.donnees, etat.derniereVie, etat.maintenant),
  maillonVeille(etat.donnees.health),
];

const GRAVITE: Record<TonMaillon, number> = { attention: 2, erreur: 3, inconnu: 1, ok: 0 };

/** Le pire ton de la chaîne — celui que l'indicateur prend (maquette B). */
export const pireTon = (liste: ReadonlyArray<Maillon>): TonMaillon =>
  liste.reduce<TonMaillon>(
    (pire, maillon) => (GRAVITE[maillon.ton] > GRAVITE[pire] ? maillon.ton : pire),
    "ok",
  );

/** Le diagnostic à copier : des états, jamais de contenu. */
export const diagnostic = (liste: ReadonlyArray<Maillon>): string =>
  liste
    .map(
      (maillon) =>
        `${maillon.titre}: ${maillon.texte}${maillon.detail === null ? "" : ` — ${maillon.detail}`}`,
    )
    .join("\n");
