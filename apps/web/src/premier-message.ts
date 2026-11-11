/**
 * Le premier message d'un agent lancé d'ici (maquette « Inbox », feuille de
 * lancement) : il **attend que l'agent soit prêt**, puis rejoint les envois.
 *
 * ⚠️ **Pourquoi attendre.** Le lancement tape la commande de l'agent dans un
 * shell ; un texte collé tout de suite tomberait dans ce shell, avant que
 * l'agent ne démarre — au mieux perdu, au pire exécuté comme une commande.
 * Le texte part quand herdr-core dit l'agent `idle` (son propre statut,
 * OSC 7501) : il attend alors une demande. Un agent bloqué (confiance du
 * dossier, connexion) n'est pas prêt : le message attend qu'on ait tranché.
 *
 * ⚠️ **Rien n'est perdu.** Tant qu'il attend, il est visible dans la
 * conversation, et « Cancel » le rend au composeur.
 *
 * Gardé en mémoire seulement, comme les envois : recharger la page le perd,
 * et la page le sait (`enAttente`).
 */
import type { AgentView } from "@herdr-core-chat/shared";

export interface PremierMessage {
  readonly pane: string;
  readonly texte: string;
  readonly creeA: number;
}

export interface MagasinPremiers {
  /** Retient le premier message d'un pane qu'on vient de lancer. */
  readonly attend: (pane: string, texte: string, creeA: number) => void;
  /** Annule l'attente ; rend le texte (pour le composeur), ou `null`. */
  readonly annule: (pane: string) => string | null;
  /** Le message qui attend sur ce pane, s'il y en a un. */
  readonly pour: (pane: string) => PremierMessage | undefined;
  /**
   * Retire et rend les messages dont l'agent est prêt (`idle`) dans cette
   * lecture de l'overview. Un pane absent de la lecture n'est pas prêt.
   */
  readonly prets: (agents: ReadonlyArray<Pick<AgentView, "paneId" | "status">>) => PremierMessage[];
  readonly enAttente: () => boolean;
  readonly souscrire: (abonne: () => void) => () => void;
  /** Change à chaque modification — la clé de `useSyncExternalStore`. */
  readonly version: () => number;
}

export const creeMagasinPremiers = (): MagasinPremiers => {
  const messages = new Map<string, PremierMessage>();
  const abonnes = new Set<() => void>();
  let version = 0;
  const change = () => {
    version += 1;
    for (const abonne of abonnes) abonne();
  };
  return {
    annule: (pane) => {
      const message = messages.get(pane);
      if (message === undefined) return null;
      messages.delete(pane);
      change();
      return message.texte;
    },
    attend: (pane, texte, creeA) => {
      const propre = texte.trim();
      if (propre.length === 0) return;
      messages.set(pane, { creeA, pane, texte: propre });
      change();
    },
    enAttente: () => messages.size > 0,
    pour: (pane) => messages.get(pane),
    prets: (agents) => {
      if (messages.size === 0) return [];
      const sortis: PremierMessage[] = [];
      for (const agent of agents) {
        const message = messages.get(agent.paneId);
        if (message !== undefined && agent.status === "idle") {
          sortis.push(message);
          messages.delete(agent.paneId);
        }
      }
      if (sortis.length > 0) change();
      return sortis;
    },
    souscrire: (abonne) => {
      abonnes.add(abonne);
      return () => abonnes.delete(abonne);
    },
    version: () => version,
  };
};
