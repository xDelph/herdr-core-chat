/**
 * Les lignes **provisoires** de la liste, tirées de l'overview.
 *
 * ⚠️ **Le démarrage à froid montrait un squelette pendant 5 à 6 s** (audit,
 * performance) alors que l'overview répondait en 3,5 ms : la liste attendait
 * `/api/conversations`, qui lit les sessions sur disque. L'overview dit déjà
 * QUI est là, dans quel état, sous quel nom : la liste s'affiche avec ça, et
 * les aperçus (dernière demande, modèle, horodatage) arrivent avec la vraie
 * liste, qui remplace celle-ci entièrement.
 *
 * Rien n'est inventé : une ligne provisoire n'a ni aperçu, ni horodatage, ni
 * question — elle se dit `available` pour ne pas afficher « No readable
 * session » sur un agent dont la session n'est simplement pas encore lue.
 * L'overview applique la même règle de visibilité que la liste
 * (`repartir_agents`, côté démon) : les mêmes agents, pas un de plus.
 */
import type { ConversationSummary } from "@herdr-core-chat/shared";
import type { Overview } from "./api.ts";

type AgentOverview = Overview["agents"][number];

const nonVide = (valeur: string | null | undefined): string | undefined => {
  const propre = valeur?.trim();
  return propre === undefined || propre.length === 0 ? undefined : propre;
};

/** Le titre, dans l'ordre du démon : titre Herdr, onglet, projet, pane. */
const titreDe = (agent: AgentOverview): string =>
  nonVide(agent.title) ?? nonVide(agent.tabLabel) ?? nonVide(agent.workspaceLabel) ?? agent.paneId;

const ligneDe = (agent: AgentOverview): ConversationSummary => ({
  agent: agent.agent,
  name: agent.name,
  paneId: agent.paneId,
  paneRank: agent.paneRank,
  sessionState: "available",
  status: agent.status,
  tabId: agent.tabId,
  tabPaneCount: agent.tabPaneCount,
  title: titreDe(agent),
  workspaceId: agent.workspaceId,
  ...(agent.blockedKind === undefined ? {} : { blockedKind: agent.blockedKind }),
  ...(agent.exit === undefined ? {} : { exit: agent.exit }),
  ...(agent.turn === undefined ? {} : { turn: agent.turn }),
  ...(agent.pinned === undefined ? {} : { pinned: agent.pinned }),
  ...(agent.subagents === undefined ? {} : { subagents: agent.subagents }),
  ...(agent.screenInexact === undefined ? {} : { screenInexact: agent.screenInexact }),
});

/**
 * Les lignes à montrer : la vraie liste dès qu'elle est lue ; avant, celles de
 * l'overview ; sans l'un ni l'autre, rien (`null` : inconnu, pas vide).
 */
export const lignesAffichees = (
  conversations: ReadonlyArray<ConversationSummary> | null,
  overview: Overview | null,
): {
  readonly lignes: ReadonlyArray<ConversationSummary> | null;
  readonly provisoires: boolean;
} => {
  if (conversations !== null) return { lignes: conversations, provisoires: false };
  if (overview === null) return { lignes: null, provisoires: false };
  return { lignes: overview.agents.map(ligneDe), provisoires: true };
};
