/**
 * Le magasin de données de l'application — **un seul propriétaire** pour la
 * liste, la santé et le flux d'événements global.
 *
 * ⚠️ **Les défauts qu'il ferme, tous mesurés dans la version précédente.**
 *
 * - Deux `EventSource` sur `/api/events` dès qu'une conversation était ouverte
 *   (un pour la liste, un pour le chat). Ici le flux global est ouvert **une
 *   fois** ; le chat s'y abonne (`ecoute`).
 * - Trois lectures `await`-ées **en série** à chaque événement : une rafale de
 *   cinq `agent_status` lançait quinze requêtes l'une après l'autre. Ici les
 *   signaux sont regroupés (`creeRegroupement`, 150 ms) et les lectures partent
 *   en **un seul vol** partagé (`creeClient`).
 * - La boucle de rafraîchissement redémarrait à chaque changement de l'état du
 *   direct. Ici la cadence est **relue à chaque tour** : le minuteur ne connaît
 *   pas l'état, il le demande.
 * - Une lecture ratée effaçait les deux autres. Ici chaque ressource garde sa
 *   dernière valeur lue et porte sa propre cause d'échec.
 * - Le démarrage attendait la plus lente des trois lectures (5 à 6 s pour la
 *   liste à froid, 3,5 ms pour l'overview). Ici chaque valeur lue est publiée
 *   **dès qu'elle arrive** (`surIssue`) ; les échecs, eux, attendent la fin du
 *   vol (ils dépendent de la visibilité de la page pendant tout le vol).
 * - Chaque rafraîchissement donnait trois rendus de toute l'app et des objets
 *   neufs même sans changement. Ici une relecture identique rend les MÊMES
 *   objets (`partage.ts`), une publication sans changement ne réveille
 *   personne, et la fin du vol est publiée en une fois.
 * - Chaque `session_changed` relisait aussi la santé, qui n'en dépend pas.
 *   Ici la santé se relit au filet, au retour, à la demande — et sur un
 *   événement seulement si elle a plus de `AGE_MAX_SANTE_MS`.
 *
 * Le module ne connaît ni React ni le réseau : tout est injecté.
 */
import type { ConversationSummary, InstalledKind } from "@herdr-core-chat/shared";
import type { Health, Overview } from "./api.ts";
import {
  cadence,
  creeClient,
  creeRegroupement,
  EVENEMENTS_RAFRAICHISSANTS,
} from "./rafraichissement.ts";
import { estInterruption } from "./interruption.ts";
import { partageStructure } from "./partage.ts";
import type { Resultat, SurIssue } from "./rafraichissement.ts";

/**
 * L'âge au-delà duquel un événement relit aussi la santé. Les événements
 * (`session_changed` surtout, à chaque écriture de session) n'ont rien à voir
 * avec elle ; le filet et le retour au premier plan la relisent toujours.
 */
export const AGE_MAX_SANTE_MS = 15_000;

export type Direct = "live" | "down" | "inconnu";

export interface EtatDonnees {
  readonly overview: Overview | null;
  readonly health: Health | null;
  readonly conversations: ReadonlyArray<ConversationSummary> | null;
  readonly erreurs: {
    readonly overview: string | null;
    readonly health: string | null;
    readonly conversations: string | null;
  };
  /** Un vol est en cours. */
  readonly chargement: boolean;
  /** Instant de la dernière lecture réussie de la liste, ou `null`. */
  readonly dernierSucces: number | null;
  readonly direct: Direct;
  /**
   * Début de travail **observé** par pane : l'instant où le flux a annoncé le
   * passage à `working`. Absent quand on ne l'a pas vu passer — on n'invente pas
   * une durée qu'on n'a pas mesurée.
   */
  readonly debutsTravail: ReadonlyMap<string, number>;
  readonly kinds: ReadonlyArray<InstalledKind> | null;
  readonly erreurKinds: string | null;
}

/** Un événement du flux global, tel que transmis aux abonnés. */
export interface EvenementGlobal {
  readonly genre: string;
  readonly donnees: unknown;
}

export interface Lectures {
  readonly overview: () => Promise<Overview>;
  readonly health: () => Promise<Health>;
  readonly conversations: () => Promise<{
    readonly conversations: ReadonlyArray<ConversationSummary>;
  }>;
  readonly kinds: () => Promise<ReadonlyArray<InstalledKind>>;
}

export type OuvreEvenements = (gestionnaires: {
  readonly onEvent: (genre: string, donnees: unknown) => void;
  readonly onResync: () => void;
  readonly onStatus: (statut: "live" | "down") => void;
  readonly onVie?: () => void;
}) => () => void;

export interface DependancesDonnees {
  readonly lectures: Lectures;
  readonly ouvreEvenements: OuvreEvenements;
  readonly planifie: (action: () => void, delai: number) => () => void;
  readonly visible: () => boolean;
  readonly maintenant: () => number;
  readonly delaiRegroupement?: number;
  /** Journalise une lecture ratée (dédoublonné par l'appelant). */
  readonly signale?: (message: string, detail?: unknown) => void;
}

export interface MagasinDonnees {
  readonly etat: () => EtatDonnees;
  readonly souscrire: (abonne: () => void) => () => void;
  /** Ouvre le flux, lance la première lecture et la cadence. Idempotent. */
  readonly demarre: () => void;
  readonly arrete: () => void;
  /**
   * Lecture immédiate. Par défaut elle rejoint le vol en cours (retour au
   * premier plan) ; `frais` exige une lecture partie APRÈS l'appel (un
   * événement, le bouton « Réessayer »).
   */
  readonly rafraichit: (frais?: boolean) => Promise<void>;
  /** S'abonner aux événements du flux global (le chat, les envois). */
  readonly ecoute: (abonne: (evenement: EvenementGlobal) => void) => () => void;
  readonly lectureDesKinds: () => Promise<void>;
  /**
   * L'instant du dernier signe de vie du flux (ouverture, battement,
   * événement), ou `null` s'il n'en a jamais donné.
   *
   * ⚠️ **Lu, jamais publié.** Chaque battement réveillerait toute l'app pour
   * une valeur que seul le centre de connexion affiche, à son propre rythme.
   */
  readonly derniereVie: () => number | null;
}

const lisChaine = (objet: unknown, cle: string): string | undefined => {
  if (typeof objet !== "object" || objet === null) return undefined;
  const valeur: unknown = Reflect.get(objet, cle);
  return typeof valeur === "string" ? valeur : undefined;
};

export const creeMagasinDonnees = (deps: DependancesDonnees): MagasinDonnees => {
  let etat: EtatDonnees = {
    chargement: false,
    conversations: null,
    debutsTravail: new Map(),
    dernierSucces: null,
    direct: "inconnu",
    erreurKinds: null,
    erreurs: { conversations: null, health: null, overview: null },
    health: null,
    kinds: null,
    overview: null,
  };
  const abonnes = new Set<() => void>();
  const ecouteurs = new Set<(evenement: EvenementGlobal) => void>();
  /** Par ressource, la génération la plus récente appliquée. */
  const appliquees = { conversations: 0, health: 0, overview: 0 };
  let demarre = false;
  let derniereVie: number | null = null;
  let fermeFlux: (() => void) | null = null;
  let annuleTour: (() => void) | null = null;

  /** Publie ce qui a CHANGÉ — rien de changé, personne n'est réveillé. */
  const publie = (changement: Partial<EtatDonnees>): void => {
    const change = Object.entries(changement).some(
      ([cle, valeur]) => !Object.is(valeur, Reflect.get(etat, cle)),
    );
    if (!change) return;
    etat = { ...etat, ...changement };
    for (const abonne of abonnes) abonne();
  };

  /*
    La santé : relue quand on la demande (filet, retour, « Réessayer »), et sur
    un événement seulement si elle a vieilli. ⚠️ La demande est COLLANTE : un
    vol déjà promis (`litFrais`) qui part plus tard la relit aussi.
  */
  let santeDemandee = true;
  let santeLue: { readonly instant: number; readonly valeur: Health } | null = null;
  const litSante = async (): Promise<Health> => {
    const recente =
      santeLue !== null && deps.maintenant() - santeLue.instant < AGE_MAX_SANTE_MS
        ? santeLue
        : null;
    if (!santeDemandee && recente !== null) return recente.valeur;
    santeDemandee = false;
    const valeur = await deps.lectures.health();
    santeLue = { instant: deps.maintenant(), valeur };
    return valeur;
  };

  type Liste = { readonly conversations: ReadonlyArray<ConversationSummary> };

  /** Une valeur lue, publiée dès son arrivée — si aucune plus récente ne l'a devancée. */
  const surIssue: SurIssue<Overview, Health, Liste> = (lue, generation) => {
    if (!lue.issue.ok || generation <= appliquees[lue.ressource]) return;
    // ⚠️ Seulement un CHANGEMENT : une valeur identique attend la fin du vol,
    // publiée en une fois avec elle (un rendu, pas deux).
    if (lue.ressource === "overview") {
      const valeur = partageStructure(etat.overview, lue.issue.valeur);
      if (valeur === etat.overview) return;
      appliquees.overview = generation;
      publie({ erreurs: { ...etat.erreurs, overview: null }, overview: valeur });
    } else if (lue.ressource === "health") {
      const valeur = partageStructure(etat.health, lue.issue.valeur);
      if (valeur === etat.health) return;
      appliquees.health = generation;
      publie({ erreurs: { ...etat.erreurs, health: null }, health: valeur });
    } else {
      const valeur = partageStructure(etat.conversations, lue.issue.valeur.conversations);
      if (valeur === etat.conversations) return;
      appliquees.conversations = generation;
      publie({
        conversations: valeur,
        dernierSucces: deps.maintenant(),
        erreurs: { ...etat.erreurs, conversations: null },
      });
    }
  };

  const client = creeClient(
    {
      conversations: deps.lectures.conversations,
      health: litSante,
      overview: deps.lectures.overview,
    },
    surIssue,
  );

  const applique = (
    resultat: Resultat<Overview, Health, Liste>,
    /** La page a été cachée pendant la lecture : un échec n'y prouve rien. */
    cachee: boolean,
  ): Partial<EtatDonnees> => {
    // ⚠️ Une réponse plus ancienne que celle déjà appliquée est ignorée, par
    // ressource : la liste ne revient jamais en arrière. (Une ressource déjà
    // publiée par ce vol, à son arrivée, n'est pas republiée.)
    const neuve = {
      conversations: resultat.generation > appliquees.conversations,
      health: resultat.generation > appliquees.health,
      overview: resultat.generation > appliquees.overview,
    };
    if (!neuve.conversations && !neuve.health && !neuve.overview) return {};
    for (const ressource of ["conversations", "health", "overview"] as const) {
      if (neuve[ressource]) appliquees[ressource] = resultat.generation;
    }
    // ⚠️ Un échec INTERROMPU (page cachée, requête coupée par iOS) n'est ni
    // journalisé ni affiché : l'erreur précédente reste ce qu'elle était, et
    // le retour au premier plan relit (`interruption.ts`).
    const echec = (issue: { readonly ok: false; readonly cause: string }) =>
      cachee || estInterruption(issue.cause) ? "interrompu" : "panne";
    // Une barre rouge disparaît au rechargement ; le journal du démon, non.
    for (const [ressource, issue] of [
      ["overview", resultat.overview],
      ["health", resultat.health],
      ["conversations", resultat.conversations],
    ] as const) {
      if (neuve[ressource] && !issue.ok && echec(issue) === "panne") {
        deps.signale?.(`failed to read ${ressource}`, { cause: issue.cause });
      }
    }
    const erreurDe = (
      issue: { readonly ok: true } | { readonly ok: false; readonly cause: string },
      precedente: string | null,
    ): string | null => {
      if (issue.ok) return null;
      return echec(issue) === "panne" ? issue.cause : precedente;
    };
    const erreurs = {
      conversations: neuve.conversations
        ? erreurDe(resultat.conversations, etat.erreurs.conversations)
        : etat.erreurs.conversations,
      health: neuve.health ? erreurDe(resultat.health, etat.erreurs.health) : etat.erreurs.health,
      overview: neuve.overview
        ? erreurDe(resultat.overview, etat.erreurs.overview)
        : etat.erreurs.overview,
    };
    const listeLue = neuve.conversations && resultat.conversations.ok;
    return {
      conversations:
        neuve.conversations && resultat.conversations.ok
          ? partageStructure(etat.conversations, resultat.conversations.valeur.conversations)
          : etat.conversations,
      dernierSucces: listeLue ? deps.maintenant() : etat.dernierSucces,
      // Les mêmes causes : le même objet (sinon chaque vol publierait).
      erreurs: partageStructure(etat.erreurs, erreurs),
      health:
        neuve.health && resultat.health.ok
          ? partageStructure(etat.health, resultat.health.valeur)
          : etat.health,
      overview:
        neuve.overview && resultat.overview.ok
          ? partageStructure(etat.overview, resultat.overview.valeur)
          : etat.overview,
    };
  };

  /**
   * @param sante relire aussi la santé — faux pour un événement, qui n'en dit
   *   rien (elle n'est alors relue que si elle a vieilli).
   */
  const rafraichit = async (frais = false, sante = true): Promise<void> => {
    if (sante) santeDemandee = true;
    publie({ chargement: true });
    const visibleAuDepart = deps.visible();
    let changement: Partial<EtatDonnees> = {};
    try {
      const resultat = await (frais ? client.litFrais() : client.lit());
      changement = applique(resultat, !visibleAuDepart || !deps.visible());
    } finally {
      // La fin du vol en UNE publication : les valeurs et la fin du chargement.
      publie({ ...changement, chargement: client.enVol() });
    }
  };

  const jetons = new Map<number, () => void>();
  let compteurJetons = 0;
  // Un événement dit qu'un état a changé : la lecture doit partir APRÈS lui.
  const regroupement = creeRegroupement(
    () => void rafraichit(true, false),
    deps.delaiRegroupement,
    (action, attente) => {
      compteurJetons += 1;
      const jeton = compteurJetons;
      jetons.set(
        jeton,
        deps.planifie(() => {
          jetons.delete(jeton);
          action();
        }, attente),
      );
      return jeton;
    },
    (jeton) => {
      jetons.get(jeton)?.();
      jetons.delete(jeton);
    },
  );
  /**
   * Le tour de fond. ⚠️ La cadence est relue **à chaque tour** : un changement
   * du direct ou de la visibilité vaut au tour suivant, sans redémarrer la
   * boucle (c'était le défaut : elle repartait à chaque `live`).
   */
  const planifieTour = (): void => {
    annuleTour?.();
    annuleTour = deps.planifie(
      () => {
        annuleTour = null;
        void rafraichit();
        planifieTour();
      },
      cadence(deps.visible(), etat.direct === "live"),
    );
  };

  const surStatut = (donnees: unknown): void => {
    const paneId = lisChaine(donnees, "paneId");
    const vers = lisChaine(donnees, "to");
    if (paneId === undefined || vers === undefined) return;
    const suivants = new Map(etat.debutsTravail);
    if (vers === "working") {
      if (!suivants.has(paneId)) suivants.set(paneId, deps.maintenant());
    } else {
      suivants.delete(paneId);
    }
    publie({ debutsTravail: suivants });
  };

  const lectureDesKinds = async (): Promise<void> => {
    try {
      publie({ erreurKinds: null, kinds: await deps.lectures.kinds() });
    } catch (cause) {
      publie({ erreurKinds: cause instanceof Error ? cause.message : String(cause) });
    }
  };

  return {
    arrete: () => {
      demarre = false;
      fermeFlux?.();
      fermeFlux = null;
      annuleTour?.();
      annuleTour = null;
      regroupement.annule();
    },
    demarre: () => {
      if (demarre) return;
      demarre = true;
      fermeFlux = deps.ouvreEvenements({
        onEvent: (genre, donnees) => {
          if (genre === "agent_status") surStatut(donnees);
          if ((EVENEMENTS_RAFRAICHISSANTS as ReadonlyArray<string>).includes(genre)) {
            regroupement.signale();
          }
          for (const ecouteur of ecouteurs) ecouteur({ donnees, genre });
        },
        onResync: () => {
          void rafraichit(true);
          for (const ecouteur of ecouteurs) ecouteur({ donnees: null, genre: "resync" });
        },
        onStatus: (statut) => {
          if (statut !== etat.direct) publie({ direct: statut });
        },
        onVie: () => {
          derniereVie = deps.maintenant();
        },
      });
      void rafraichit();
      void lectureDesKinds();
      planifieTour();
    },
    ecoute: (abonne) => {
      ecouteurs.add(abonne);
      return () => ecouteurs.delete(abonne);
    },
    derniereVie: () => derniereVie,
    etat: () => etat,
    lectureDesKinds,
    rafraichit,
    souscrire: (abonne) => {
      abonnes.add(abonne);
      return () => abonnes.delete(abonne);
    },
  };
};
